use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use crate::license::LicenseManager;
use crate::photon::ChatMessage;
use crate::sniffer::PacketSniffer;
use crate::translator::TranslationEngine;

pub struct AppState {
    pub sniffer: Arc<Mutex<PacketSniffer>>,
    pub translator: Arc<Mutex<TranslationEngine>>,
    pub license: Arc<Mutex<LicenseManager>>,
}

impl AppState {
    pub fn new(tx: mpsc::Sender<ChatMessage>) -> Self {
        Self {
            sniffer: Arc::new(Mutex::new(PacketSniffer::new(tx))),
            translator: Arc::new(Mutex::new(TranslationEngine::new())),
            license: Arc::new(Mutex::new(LicenseManager::new())),
        }
    }
}
