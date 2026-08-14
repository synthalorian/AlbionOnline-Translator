use pcap::{Capture, Device};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::{network::{extract_udp_payload, ip_in_cidr}, photon::{ChatChannel, PhotonDecoder}, translator::TranslationEngine};
use crate::photon;

// Albion Online server IP ranges.
const ALBION_CIDRS: [&str; 1] = ["5.188.125.0/24"];
const ALBION_UDP_PORTS: [u16; 2] = [5056, 4535];

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

        let mut cap = Capture::from_device(device)
            .map_err(|e| SnifferError::CaptureOpen(e.to_string()))?
            .promisc(true)
            .snaplen(65535)
            .timeout(1000)
            .open()
            .map_err(|e| SnifferError::CaptureOpen(e.to_string()))?;

        cap.filter("udp port 5056 or udp port 4535", true)
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

            let decoder = PhotonDecoder::new();
            let mut packet_number = 0usize;
            let mut filtered_count = 0usize;

            while running.load(Ordering::SeqCst) {
                match cap.next_packet() {
                    Ok(packet) => {
                        packet_number += 1;

                        // Extract UDP payload from raw ethernet frame.
                        if let Some((src_ip, dst_ip, payload)) =
                            extract_udp_payload(packet.data)
                        {
                            // Filter to Albion server IPs only.
                            if !ALBION_CIDRS.iter().any(|cidr| {
                                ip_in_cidr(src_ip, cidr) || ip_in_cidr(dst_ip, cidr)
                            }) {
                                filtered_count += 1;
                                continue;
                            }

                            if let Some(msg) = decoder.decode(payload) {
                                // Bounded channel = backpressure: if the translator
                                // is busy, drop further processing instead of
                                // queueing unbounded work.
                                if raw_tx.send(msg).await.is_err() {
                                    error!("Failed to send chat message");
                                    break;
                                }
                            }
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
