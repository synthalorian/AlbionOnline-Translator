//! License management — Lemon Squeezy one-time license keys.
//!
//! Modes:
//!   - Trial:    7 days from first launch, full features
//!   - Licensed: key activated + validated against Lemon Squeezy
//!   - Locked:   trial expired with no valid license
//!
//! Lemon Squeezy's license endpoints (/activate, /validate, /deactivate)
//! do NOT require an API token — only the customer's license key — so they
//! are safe to call from a distributed client.
//!
//! Offline policy: a validated license stays valid offline for
//! OFFLINE_GRACE_DAYS since the last successful server validation.

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

const LS_API_BASE: &str = "https://api.lemonsqueezy.com/v1/licenses";

/// TODO(synth): replace with the real checkout URL after creating the
/// Lemon Squeezy product (see docs/MONETIZATION.md).
pub const BUY_URL: &str = "https://synthalorian.lemonsqueezy.com/checkout/buy/REPLACE_ME";

const TRIAL_DAYS: i64 = 7;
const OFFLINE_GRACE_DAYS: i64 = 7;
const REVALIDATE_AFTER_HOURS: i64 = 24;

// ---------------------------------------------------------------------------
// Persistent state (stored as JSON in the app config dir)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoredLicense {
    license_key: Option<String>,
    instance_id: Option<String>,
    license_status: Option<String>,
    last_validated: Option<DateTime<Utc>>,
    first_seen: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Status reported to the frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum LicenseMode {
    Trial { days_remaining: i64 },
    Licensed { status: String },
    Locked,
}

#[derive(Debug, Clone, Serialize)]
pub struct LicenseStatus {
    #[serde(flatten)]
    pub mode: LicenseMode,
    pub buy_url: String,
}

impl LicenseStatus {
    fn new(mode: LicenseMode) -> Self {
        Self {
            mode,
            buy_url: BUY_URL.to_string(),
        }
    }

    pub fn is_unlocked(&self) -> bool {
        !matches!(self.mode, LicenseMode::Locked)
    }
}

// ---------------------------------------------------------------------------
// Lemon Squeezy API shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LsResponse {
    success: Option<bool>,
    error: Option<String>,
    license_key: Option<LsLicenseKey>,
    instance: Option<LsInstance>,
}

#[derive(Debug, Deserialize)]
struct LsLicenseKey {
    status: String,
}

#[derive(Debug, Deserialize)]
struct LsInstance {
    id: String,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

pub struct LicenseManager {
    stored: StoredLicense,
    path: PathBuf,
    client: reqwest::Client,
    locked_notice_sent: bool,
}

impl LicenseManager {
    pub fn new() -> Self {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("albion-translator")
            .join("license.json");

        let mut stored: StoredLicense = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        // Stamp the trial clock on first ever launch
        if stored.first_seen.is_none() {
            stored.first_seen = Some(Utc::now());
        }

        let mgr = Self {
            stored,
            path,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            locked_notice_sent: false,
        };
        mgr.save();
        mgr
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.stored) {
            let _ = std::fs::write(&self.path, json);
        }
    }

    /// Activate a license key against Lemon Squeezy.
    pub async fn activate(&mut self, key: &str) -> Result<LicenseStatus, String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("License key cannot be empty".to_string());
        }

        let instance_name = format!("{}-{}", whoami_hostname(), &uuid_short());

        let resp = self
            .client
            .post(format!("{}/activate", LS_API_BASE))
            .form(&[("license_key", key), ("instance_name", &instance_name)])
            .send()
            .await
            .map_err(|e| format!("Network error — check your connection: {}", e))?;

        let body: LsResponse = resp
            .json()
            .await
            .map_err(|e| format!("Bad response from licensing server: {}", e))?;

        if body.success != Some(true) {
            return Err(body
                .error
                .unwrap_or_else(|| "Invalid license key".to_string()));
        }

        let instance = body
            .instance
            .ok_or_else(|| "Licensing server returned no instance".to_string())?;
        let status = body
            .license_key
            .map(|k| k.status)
            .unwrap_or_else(|| "active".to_string());

        self.stored.license_key = Some(key.to_string());
        self.stored.instance_id = Some(instance.id);
        self.stored.license_status = Some(status.clone());
        self.stored.last_validated = Some(Utc::now());
        self.save();

        tracing::info!("License activated (status: {})", status);
        Ok(LicenseStatus::new(LicenseMode::Licensed { status }))
    }

    /// Current status. Revalidates online when stale; applies offline grace.
    pub async fn status(&mut self) -> LicenseStatus {
        if self.stored.license_key.is_some() {
            self.licensed_status().await
        } else {
            self.trial_status()
        }
    }

    async fn licensed_status(&mut self) -> LicenseStatus {
        let now = Utc::now();
        let last = self.stored.last_validated;

        let stale = last
            .map(|t| now - t > Duration::hours(REVALIDATE_AFTER_HOURS))
            .unwrap_or(true);

        if stale {
            match self.validate_online().await {
                Ok(status) => {
                    self.stored.license_status = Some(status.clone());
                    self.stored.last_validated = Some(now);
                    self.save();
                    return LicenseStatus::new(LicenseMode::Licensed { status });
                }
                Err(NetworkOrStatus::Server(status)) => {
                    // Server reachable, key genuinely dead (refunded/disabled)
                    tracing::warn!("License no longer valid: {}", status);
                    return LicenseStatus::new(LicenseMode::Locked);
                }
                Err(NetworkOrStatus::Network(e)) => {
                    tracing::warn!("License revalidation offline: {}", e);
                    // fall through to grace check
                }
            }
        }

        // Fresh enough, or offline: honor grace window
        let within_grace = last
            .map(|t| now - t <= Duration::days(OFFLINE_GRACE_DAYS))
            .unwrap_or(false);

        if within_grace {
            let status = self
                .stored
                .license_status
                .clone()
                .unwrap_or_else(|| "active".to_string());
            LicenseStatus::new(LicenseMode::Licensed { status })
        } else {
            // Never validated and can't reach the server — can't trust it
            LicenseStatus::new(LicenseMode::Locked)
        }
    }

    fn trial_status(&self) -> LicenseStatus {
        let first = self.stored.first_seen.unwrap_or_else(Utc::now);
        let elapsed = Utc::now() - first;
        let remaining = TRIAL_DAYS - elapsed.num_days();
        if remaining >= 0 {
            LicenseStatus::new(LicenseMode::Trial {
                days_remaining: remaining,
            })
        } else {
            LicenseStatus::new(LicenseMode::Locked)
        }
    }

    async fn validate_online(&self) -> Result<String, NetworkOrStatus> {
        let key = self.stored.license_key.as_deref().unwrap_or_default();
        let instance = self.stored.instance_id.as_deref().unwrap_or_default();

        let resp = self
            .client
            .post(format!("{}/validate", LS_API_BASE))
            .form(&[("license_key", key), ("instance_id", instance)])
            .send()
            .await
            .map_err(|e| NetworkOrStatus::Network(e.to_string()))?;

        let body: LsResponse = resp
            .json()
            .await
            .map_err(|e| NetworkOrStatus::Network(e.to_string()))?;

        if body.success == Some(true) {
            Ok(body
                .license_key
                .map(|k| k.status)
                .unwrap_or_else(|| "active".to_string()))
        } else {
            Err(NetworkOrStatus::Server(
                body.error
                    .unwrap_or_else(|| "inactive".to_string()),
            ))
        }
    }

    /// Deactivate this machine's seat (frees the activation for another PC).
    pub async fn deactivate(&mut self) -> Result<(), String> {
        let key = self.stored.license_key.clone();
        let instance = self.stored.instance_id.clone();

        if let (Some(key), Some(instance)) = (key, instance) {
            let _ = self
                .client
                .post(format!("{}/deactivate", LS_API_BASE))
                .form(&[("license_key", key.as_str()), ("instance_id", instance.as_str())])
                .send()
                .await; // best-effort
        }

        self.stored.license_key = None;
        self.stored.instance_id = None;
        self.stored.license_status = None;
        self.stored.last_validated = None;
        self.save();
        Ok(())
    }

    /// Gate check for the hot path (message forwarding).
    pub async fn is_unlocked(&mut self) -> bool {
        self.status().await.is_unlocked()
    }

    /// Whether we've already told the frontend we're locked (avoid event spam).
    pub fn take_locked_notice(&mut self) -> bool {
        if self.locked_notice_sent {
            false
        } else {
            self.locked_notice_sent = true;
            true
        }
    }
}

enum NetworkOrStatus {
    Network(String),
    Server(String),
}

fn whoami_hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "albion-pc".to_string())
}

fn uuid_short() -> String {
    // Cheap instance discriminator without adding a uuid dep
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    Utc::now().timestamp_nanos_opt().hash(&mut h);
    std::process::id().hash(&mut h);
    format!("{:08x}", h.finish() as u32)
}
