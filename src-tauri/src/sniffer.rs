use pcap::{Capture, Device};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::{network::extract_udp_payload, photon::PhotonDecoder, translator::TranslationEngine, hosts::HostFilter};
use crate::photon;

// Albion's game UDP ports are stable across the entire server fleet, while
// the server IPs rotate across many ranges (5.188.125.x, 5.45.187.x,
// 193.169.238.x, 85.234.70.x all observed live), so an IP whitelist silently
// drops chat from any range not listed. Filter on ports at the BPF level
// (fast, kernel-side) and on IPs after extraction (user-side, based on
// hosts.txt CIDR ranges).
const BPF_FILTER: &str = "udp port 5055 or udp port 5056 or udp port 4535";

const DEFAULT_HOSTS_PATH: &str = "hosts.txt";

pub struct PacketSniffer {
    running: Arc<AtomicBool>,
    tx: mpsc::Sender<photon::ChatMessage>,
    host_filter: Option<HostFilter>,
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

        Self {
            running: Arc::new(AtomicBool::new(false)),
            tx,
            host_filter,
        }
    }

    pub fn start(&mut self) -> Result<(), SnifferError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(SnifferError::AlreadyRunning);
        }

        // Pick the interface that owns the default route — Device::lookup()
        // happily returns tailscale0/lo and silently captures nothing.
        let route_dev = std::process::Command::new("ip")
            .args(["-4", "route", "show", "default"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                s.split_whitespace()
                    .skip_while(|&w| w != "dev")
                    .nth(1)
                    .map(|w| w.to_string())
            });

        let device = Device::list()
            .map_err(|e| SnifferError::DeviceLookup(e.to_string()))?
            .into_iter()
            .find(|d| Some(&d.name) == route_dev.as_ref())
            .or_else(|| Device::lookup().ok().flatten())
            .ok_or(SnifferError::NoDevice)?;

        info!("Using device: {}", device.name);

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
        let running = self.running.clone();
        let tx = self.tx.clone();
        let host_filter = self.host_filter.clone();

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

            let mut decoder = PhotonDecoder::new();
            let mut packet_number = 0usize;
            let mut filtered_count = 0usize;
            let mut ip_filtered_count = 0usize;

            while running.load(Ordering::SeqCst) {
                match cap.next_packet() {
                    Ok(packet) => {
                        packet_number += 1;

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

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
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

        // Only skip when we're confident it's already the target language.
        // When lingua can't decide (guild spam full of tags/symbols), still
        // translate — Google's sl=auto detects on its side and handles the
        // noisy texts lingua gives up on.
        let should_translate = source_lang
            .as_deref()
            .map(|src| src != translator.target_language())
            .unwrap_or(true);

        let translated_text = if should_translate {
            translator.translate(&msg.text, source_lang.as_deref()).await
        } else {
            None
        };

        photon::ChatMessage {
            timestamp: msg.timestamp.clone(),
            channel: msg.channel.clone(),
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
