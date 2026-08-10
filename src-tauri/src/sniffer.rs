use pcap::{Capture, Device, Packet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::photon::{ChatMessage, PhotonDecoder};

const ALBION_PORT: u16 = 5056;

pub struct PacketSniffer {
    running: Arc<AtomicBool>,
    tx: mpsc::Sender<ChatMessage>,
    decoder: PhotonDecoder,
}

impl PacketSniffer {
    pub fn new(tx: mpsc::Sender<ChatMessage>) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            tx,
            decoder: PhotonDecoder::new(),
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

        cap.filter(&format!("udp port {}", ALBION_PORT), true)
            .map_err(|e| SnifferError::Filter(e.to_string()))?;

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let tx = self.tx.clone();
        let decoder = self.decoder.clone();

        tokio::spawn(async move {
            info!("Packet capture started on port {}", ALBION_PORT);
            
            while running.load(Ordering::SeqCst) {
                match cap.next_packet() {
                    Ok(packet) => {
                        if let Some(msg) = Self::process_packet(&packet, &decoder) {
                            if tx.send(msg).await.is_err() {
                                error!("Failed to send chat message to channel");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        if running.load(Ordering::SeqCst) {
                            debug!("Packet capture timeout or error: {}", e);
                        }
                    }
                }
            }
            
            info!("Packet capture stopped");
        });

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn process_packet(packet: &Packet, decoder: &PhotonDecoder) -> Option<ChatMessage> {
        // Skip Ethernet header (14 bytes) + IP header (20 bytes min) + UDP header (8 bytes)
        // This is a simplified approach - real implementation needs proper IP header parsing
        let data = packet.data;
        
        if data.len() < 42 {
            return None;
        }

        // Extract UDP payload (simplified - assumes no IP options)
        let udp_payload = &data[42..];
        
        // Decode Photon packet
        decoder.decode(udp_payload)
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
