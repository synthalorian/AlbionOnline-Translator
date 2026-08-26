use pcap::{Capture, Device};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::{network::extract_udp_payload, photon::PhotonDecoder, translator::TranslationEngine, hosts::HostFilter};
use crate::photon::{self, ChatChannel};

// Albion's game UDP ports are stable across the entire server fleet, while
// the server IPs rotate across many ranges (5.188.125.x, 5.45.187.x,
// 193.169.238.x, 85.234.70.x all observed live), so an IP whitelist silently
// drops chat from any range not listed. Filter on ports at the BPF level
// (fast, kernel-side) and on IPs after extraction (user-side, based on
// hosts.txt CIDR ranges).
const BPF_FILTER: &str = "udp port 5055 or udp port 5056 or udp port 4535";

const DEFAULT_HOSTS_PATH: &str = "hosts.txt";

/// Channel mappings persist across app restarts (same game session = same ids).
/// Saved to ~/.config/albion-translator/channels.json.
fn channel_map_path() -> std::path::PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("albion-translator");
    std::fs::create_dir_all(&dir).ok();
    dir.join("channels.json")
}

pub fn save_channel_map(map: &HashMap<i64, ChatChannel>) {
    let path = channel_map_path();
    // Only save non-Unknown mappings — no point persisting noise
    let filtered: HashMap<String, String> = map
        .iter()
        .filter(|(_, ch)| **ch != ChatChannel::Unknown && **ch != ChatChannel::Language)
        .map(|(id, ch)| (id.to_string(), ch.to_string()))
        .collect();
    if let Ok(json) = serde_json::to_string_pretty(&filtered) {
        std::fs::write(&path, json).ok();
    }
}

pub fn load_channel_map() -> HashMap<i64, ChatChannel> {
    let path = channel_map_path();
    let Ok(json) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let Ok(raw) = serde_json::from_str::<HashMap<String, String>>(&json) else {
        return HashMap::new();
    };
    raw.iter()
        .filter_map(|(id, ch)| {
            let id = id.parse::<i64>().ok()?;
            let channel = match ch.as_str() {
                "Local" => ChatChannel::Say,
                "Whisper" => ChatChannel::Whisper,
                "Party" => ChatChannel::Party,
                "Guild" => ChatChannel::Guild,
                "Alliance" => ChatChannel::Alliance,
                "Global" => ChatChannel::Global,
                "Trade" => ChatChannel::Trade,
                "LFG" => ChatChannel::LFG,
                "Recruitment" => ChatChannel::Recruitment,
                "Faction" => ChatChannel::Faction,
                _ => return None,
            };
            Some((id, channel))
        })
        .collect()
}

/// The machine's primary outbound IPv4, found by "connecting" a UDP socket —
/// no traffic is actually sent, the kernel just resolves the route and tells
/// us which local address it would use. Cross-platform replacement for
/// parsing `ip route` (which doesn't exist on Windows).
fn primary_outbound_ip() -> Option<IpAddr> {
    let sock = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    sock.connect(("8.8.8.8", 80)).ok()?;
    sock.local_addr().ok().map(|a| a.ip())
}

/// Human-readable label for a pcap device: description (friendly on Windows,
/// e.g. "Realtek PCIe GbE Family Controller") falling back to the raw name.
fn device_label(dev: &Device) -> String {
    match &dev.desc {
        Some(d) if !d.is_empty() => format!("{} ({})", d, dev.name),
        _ => dev.name.clone(),
    }
}

/// List all capturable devices with their addresses — for diagnostics and a
/// future manual picker.
pub fn list_devices() -> Vec<String> {
    Device::list()
        .map(|devs| {
            devs.iter()
                .map(|d| {
                    let addrs: Vec<String> =
                        d.addresses.iter().map(|a| a.addr.to_string()).collect();
                    format!("{} [{}]", device_label(d), addrs.join(", "))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub struct PacketSniffer {
    running: Arc<AtomicBool>,
    packets: Arc<AtomicU64>,
    tx: mpsc::Sender<photon::ChatMessage>,
    host_filter: Option<HostFilter>,
    /// Shared channel map — the decoder reads/writes through this, and the
    /// Tauri command handler can inject manual mappings from the UI.
    channel_map: Arc<StdMutex<HashMap<i64, ChatChannel>>>,
}

impl PacketSniffer {
    pub fn new(tx: mpsc::Sender<photon::ChatMessage>) -> Self {
        // Try to load hosts.txt from the current working directory; if it
        // doesn't exist or is empty, run unfiltered (backward compat).
        let host_filter = if std::path::Path::new(DEFAULT_HOSTS_PATH).exists() {
            HostFilter::from_file(std::path::Path::new(DEFAULT_HOSTS_PATH)).ok()
        } else {
            None
        };

        // Load persisted channel mappings from previous sessions
        let saved = load_channel_map();
        if !saved.is_empty() {
            info!("Loaded {} saved channel mappings", saved.len());
        }

        Self {
            running: Arc::new(AtomicBool::new(false)),
            packets: Arc::new(AtomicU64::new(0)),
            tx,
            host_filter,
            channel_map: Arc::new(StdMutex::new(saved)),
        }
    }

    /// Start capture. Returns a human-readable label of the device being
    /// listened on, so the UI can show exactly where packets come from.
    pub fn start(&mut self) -> Result<String, SnifferError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(SnifferError::AlreadyRunning);
        }

        // Pick the interface that owns the machine's primary outbound IP —
        // Device::lookup() happily returns a VPN/loopback/virtual adapter and
        // silently captures nothing, and Windows has no `ip route` to parse.
        // The UDP-connect trick works identically on Linux and Windows.
        let primary_ip = primary_outbound_ip();
        if let Some(ip) = primary_ip {
            info!("Primary outbound IP: {}", ip);
        }

        let devices = Device::list().map_err(|e| SnifferError::DeviceLookup(e.to_string()))?;
        let device = devices
            .iter()
            .find(|d| d.addresses.iter().any(|a| Some(a.addr) == primary_ip))
            .cloned()
            .or_else(|| Device::lookup().ok().flatten())
            .ok_or(SnifferError::NoDevice)?;

        let label = device_label(&device);
        info!("Using device: {}", label);

        if let Some(ref hf) = self.host_filter {
            info!(
                "IP filtering active: {} CIDR range(s)",
                hf.len()
            );
        } else {
            info!("IP filtering disabled — all UDP on Albion ports will be processed");
        }

        let mut cap = None;
        // cargo rebuilds replace the binary and wipe its setcap caps, so a
        // freshly relaunched app can hit a brief permission window. Retry
        // instead of failing the capture outright.
        for attempt in 1..=5 {
            let inactive = match Capture::from_device(device.clone()) {
                Ok(d) => d,
                Err(e) => return Err(SnifferError::CaptureOpen(e.to_string())),
            };
            match inactive
                .promisc(true)
                .snaplen(65535)
                .timeout(1000)
                .open()
            {
                Ok(c) => {
                    cap = Some(c);
                    break;
                }
                Err(e) => {
                    if attempt == 5 {
                        return Err(SnifferError::CaptureOpen(e.to_string()));
                    }
                    error!("Capture open failed (attempt {}): {} — retrying", attempt, e);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
        let mut cap = cap.expect("capture open should succeed after retries");

        cap.filter(BPF_FILTER, true)
            .map_err(|e| SnifferError::Filter(e.to_string()))?;

        self.running.store(true, Ordering::SeqCst);
        self.packets.store(0, Ordering::SeqCst);
        let running = self.running.clone();
        let packets = self.packets.clone();
        let tx = self.tx.clone();
        let host_filter = self.host_filter.clone();
        let channel_map = self.channel_map.clone();

        // Raw decoded (untranslated) messages flow into a bounded channel;
        // a dedicated worker translates them off the capture loop so a slow
        // network call can never stall packet sniffing.
        let (raw_tx, mut raw_rx) = mpsc::channel::<photon::ChatMessage>(64);

        tokio::spawn(async move {
            debug!("translation worker: spawned");
            let mut translator = TranslationEngine::new();
            debug!("translation worker: engine ready, entering recv loop");
            while let Some(msg) = raw_rx.recv().await {
                debug!("translation worker: received msg from {}", msg.sender);
                let ui_msg = Self::convert_message(&msg, &mut translator).await;
                if tx.send(ui_msg).await.is_err() {
                    break;
                }
            }
            debug!("translation worker: recv loop exited");
        });

        // CRITICAL: this loop MUST live on the blocking thread pool, not in a
        // tokio::spawn task. pcap's next_packet() blocks the thread
        // synchronously, so the capture task's poll() never returns Pending —
        // it pins a runtime worker forever. The mpsc receiver woken by send()
        // lands in that pinned worker's local queue and is never polled:
        // translation silently starves while decoding logs look healthy.
        // spawn_blocking + blocking_send is the correct shape for a
        // synchronous capture source.
        tokio::task::spawn_blocking(move || {
            info!("Packet capture started");

            let mut decoder = PhotonDecoder::with_channel_map(channel_map);
            let mut packet_number = 0usize;
            let mut filtered_count = 0usize;
            let mut ip_filtered_count = 0usize;

            while running.load(Ordering::SeqCst) {
                match cap.next_packet() {
                    Ok(packet) => {
                        packet_number += 1;
                        packets.store(packet_number as u64, Ordering::SeqCst);

                        // Extract UDP payload from raw ethernet frame. The BPF
                        // port filter already gates on Albion's ports; the
                        // decoder below validates structure, so non-Albion
                        // traffic never survives to the channel.
                        if let Some((src_ip, dst_ip, payload)) = extract_udp_payload(packet.data) {
                            // Apply IP-based host filtering (hosts.txt CIDR ranges).
                            // Match either endpoint: inbound chat has the server as
                            // src, but outbound whispers have it as dst — checking
                            // only src would silently drop everything you send.
                            if let Some(ref hf) = host_filter {
                                if !hf.contains(src_ip) && !hf.contains(dst_ip) {
                                    ip_filtered_count += 1;
                                    continue;
                                }
                            }

                            if let Some(msg) = decoder.decode(payload) {
                                // blocking_send waits for capacity — natural
                                // backpressure straight to the capture loop.
                                if raw_tx.blocking_send(msg).is_err() {
                                    error!("Failed to send chat message");
                                    break;
                                }
                            }
                        } else {
                            // Packet passed the port filter but isn't extractable
                            // IP/UDP — count it so the stop log stays diagnostic.
                            filtered_count += 1;
                        }
                    }
                    Err(e) => {
                        if running.load(Ordering::SeqCst) {
                            debug!("Capture timeout: {}", e);
                        }
                    }
                }
            }

            info!(
                "Packet capture stopped. Total: {}, Filtered (non-IP): {}, IP-filtered: {}",
                packet_number, filtered_count, ip_filtered_count
            );
        });

        Ok(label)
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Packets captured since the current capture session started.
    pub fn packet_count(&self) -> u64 {
        self.packets.load(Ordering::SeqCst)
    }

    /// Inject a manual channel mapping from the UI (e.g. user tags Unknown
    /// channel 25813 as Guild). Takes effect immediately for future messages.
    /// Persisted to disk so it survives app restarts within the same game session.
    pub fn set_channel_mapping(&self, channel_id: i64, channel: ChatChannel) {
        if let Ok(mut map) = self.channel_map.lock() {
            info!("Manual channel mapping: {} -> {}", channel_id, channel);
            map.insert(channel_id, channel);
            save_channel_map(&map);
        }
    }

    /// Get the shared channel map for the decoder.
    pub fn shared_channel_map(&self) -> Arc<StdMutex<HashMap<i64, ChatChannel>>> {
        self.channel_map.clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    async fn convert_message(msg: &photon::ChatMessage, translator: &mut TranslationEngine) -> photon::ChatMessage {
        let source_lang = translator.detect_language(&msg.text);
        debug!(
            "convert_message: sender={} detect={:?} text_len={}",
            msg.sender,
            source_lang,
            msg.text.len()
        );

        // translate() self-gates: it returns None only when lingua is
        // CONFIDENT the text is already the target language. Uncertain,
        // mixed, or unsupported-language chat goes to Google sl=auto, so
        // nothing gets silently dropped here anymore.
        let translated_text = translator.translate(&msg.text, source_lang.as_deref()).await;

        photon::ChatMessage {
            timestamp: msg.timestamp.clone(),
            channel: msg.channel.clone(),
            channel_id: msg.channel_id,
            sender: msg.sender.clone(),
            text: msg.text.clone(),
            source_lang,
            translated_text,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SnifferError {
    #[error("Capture already running")]
    AlreadyRunning,
    #[error("No network device found")]
    NoDevice,
    #[error("Failed to lookup device: {0}")]
    DeviceLookup(String),
    #[error("Failed to open capture: {0}")]
    CaptureOpen(String),
    #[error("Failed to set filter: {0}")]
    Filter(String),
}

#[cfg(test)]
mod device_selection_tests {
    use super::*;

    /// The UDP-connect trick must yield a real local IPv4, and that IP
    /// must belong to exactly one capturable device — the selection logic
    /// start() relies on. Device::list() works unprivileged on Linux/Windows.
    #[test]
    fn primary_ip_matches_a_capture_device() {
        let ip =
            primary_outbound_ip().expect("UDP-connect trick must resolve an outbound IP");
        assert!(ip.is_ipv4(), "expected IPv4 primary address, got {}", ip);
        assert!(!ip.is_loopback(), "primary IP must not be loopback");

        let devices = Device::list().expect("Device::list must work unprivileged");
        let matches: Vec<_> = devices
            .iter()
            .filter(|d| d.addresses.iter().any(|a| a.addr == ip))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "primary IP {} must match exactly one device; found {:?} among {}",
            ip,
            matches.iter().map(|d| &d.name).collect::<Vec<_>>(),
            list_devices().join(" | ")
        );
    }
}
