use pcap::{Capture, Device};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use albion_network_lib::{
    DecodedPacket, ExtractedPacket, HostFilter, PhotonParser, PhotonParserConfig,
    extract_udp_payload,
};

use crate::photon::{ChatMessage as UiChatMessage, ChatChannel as UiChatChannel};
use crate::translator::TranslationEngine;

// Albion Online server IP ranges (from albion-translator hosts.txt)
const ALBION_CIDRS: &[&str] = &[
    "5.188.125.0/24",
];

pub struct PacketSniffer {
    running: Arc<AtomicBool>,
    tx: mpsc::Sender<UiChatMessage>,
}

impl PacketSniffer {
    pub fn new(tx: mpsc::Sender<UiChatMessage>) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            tx,
        }
    }

    pub fn start(&mut self) -> Result<(), SnifferError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(SnifferError::AlreadyRunning);
        }

        let device = Device::lookup()
            .map_err(|e| SnifferError::DeviceLookup(e.to_string()))?
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

        tokio::spawn(async move {
            info!("Packet capture started");
            
            // Build host filter for Albion servers
            let host_filter = HostFilter::from_cidrs(ALBION_CIDRS.iter().map(|s| *s))
                .expect("Invalid CIDR range");
            info!("Filtering to {} Albion server ranges", host_filter.len());
            
            let config = PhotonParserConfig::with_defaults("live".to_string(), false);
            let mut parser = PhotonParser::new(config);
            let mut translator = TranslationEngine::new();
            let mut packet_number = 0usize;
            let mut filtered_count = 0usize;
            
            while running.load(Ordering::SeqCst) {
                match cap.next_packet() {
                    Ok(packet) => {
                        packet_number += 1;
                        
                        if let Some(udp_packet) = extract_udp_payload(packet.data, None) {
                            // Filter to Albion servers only
                            if !host_filter.contains(udp_packet.source.ip) 
                                && !host_filter.contains(udp_packet.destination.ip) {
                                filtered_count += 1;
                                continue;
                            }
                            
                            let before = parser.decoded_packets().len();
                            let _ = parser.receive_packet(
                                udp_packet.payload,
                                packet_number,
                                udp_packet.source,
                                udp_packet.destination,
                            );
                            
                            // Process newly decoded packets
                            for decoded in &parser.decoded_packets()[before..] {
                                if let DecodedPacket::Event(event) = decoded {
                                    if let Some(ExtractedPacket::ChatMessage(_)) = &event.extracted {
                                        let ui_msg = Self::convert_message(&event.extracted, &mut translator).await;
                                        if tx.send(ui_msg).await.is_err() {
                                            error!("Failed to send chat message");
                                            break;
                                        }
                                    }
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
            
            info!("Packet capture stopped. Total: {}, Filtered: {}", packet_number, filtered_count);
        });

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    async fn convert_message(extracted: &Option<ExtractedPacket>, translator: &mut TranslationEngine) -> UiChatMessage {
        let now = chrono::Local::now();
        
        // Serialize to JSON to extract fields
        let json = serde_json::to_value(extracted).unwrap_or_default();
        
        let player_name = json.get("player_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        
        let message = json.get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        
        let channel_type = json.get("channel_type")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        
        // Detect language
        let source_lang = translator.detect_language(&message);
        
        // Translate if needed
        let translated_text = if let Some(ref src) = source_lang {
            if src != translator.target_language() {
                translator.translate(&message, Some(src)).await
            } else {
                None
            }
        } else {
            None
        };
        
        UiChatMessage {
            timestamp: now.format("%H:%M:%S").to_string(),
            channel: Self::map_channel(channel_type),
            sender: player_name,
            text: message,
            source_lang,
            translated_text,
        }
    }

    fn map_channel(channel: &str) -> UiChatChannel {
        match channel {
            "Say" | "Local" => UiChatChannel::Say,
            "Guild" => UiChatChannel::Guild,
            "Faction" => UiChatChannel::Faction,
            "Whisper" => UiChatChannel::Whisper,
            "Party" => UiChatChannel::Party,
            "Alliance" => UiChatChannel::Alliance,
            "Global" => UiChatChannel::Global,
            "Trade" => UiChatChannel::Trade,
            _ => UiChatChannel::Unknown,
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
