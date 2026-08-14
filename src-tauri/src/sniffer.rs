use pcap::{Capture, Device};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::{network::extract_udp_payload, photon::PhotonDecoder, translator::TranslationEngine};
use crate::photon;

// Albion's game UDP ports are stable across the entire server fleet, while
// the server IPs rotate across many ranges (5.188.125.x, 5.45.187.x,
// 193.169.238.x, 85.234.70.x all observed live), so an IP whitelist silently
// drops chat from any range not listed. Filter on ports instead; the decoder
// still validates payloads, so unrelated traffic on these ports is dropped.
const ALBION_UDP_FILTER: &str = "udp port 5055 or udp port 5056 or udp port 4535";

pub struct PacketSniffer {
    running: Arc<AtomicBool>,
    tx: mpsc::Sender<photon::ChatMessage>,
}

impl PacketSniffer {
    pub fn new(tx: mpsc::Sender<photon::ChatMessage>) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            tx,
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

        // See ALBION_UDP_FILTER: the fleet's ports are stable, its IPs are
        // not, so filter on ports.
        cap.filter(ALBION_UDP_FILTER, true)
            .map_err(|e| SnifferError::Filter(e.to_string()))?;

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let tx = self.tx.clone();

        // Raw decoded (untranslated) messages flow into a bounded channel;
        // a dedicated worker translates them off the capture loop so a slow
        // network call can never stall packet sniffing.
        let (raw_tx, mut raw_rx) = mpsc::channel::<photon::ChatMessage>(64);

        tokio::spawn(async move {
            let mut translator = TranslationEngine::new();
            while let Some(msg) = raw_rx.recv().await {
                let ui_msg = Self::convert_message(&msg, &mut translator).await;
                if tx.send(ui_msg).await.is_err() {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            info!("Packet capture started");

            let mut decoder = PhotonDecoder::new();
            let mut packet_number = 0usize;
            let mut filtered_count = 0usize;

            while running.load(Ordering::SeqCst) {
                match cap.next_packet() {
                    Ok(packet) => {
                        packet_number += 1;

                        // Extract UDP payload from raw ethernet frame. The BPF
                        // port filter already gates on Albion's ports; the
                        // decoder below validates structure, so non-Albion
                        // traffic never survives to the channel.
                        if let Some((_, _, payload)) = extract_udp_payload(packet.data) {
                            if let Some(msg) = decoder.decode(payload) {
                                // Bounded channel = backpressure: if the translator
                                // is busy, drop further processing instead of
                                // queueing unbounded work.
                                if raw_tx.send(msg).await.is_err() {
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
                "Packet capture stopped. Total: {}, Filtered: {}",
                packet_number, filtered_count
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

        let translated_text = if let Some(ref src) = source_lang {
            if src != translator.target_language() {
                translator.translate(&msg.text, Some(src)).await
            } else {
                None
            }
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
