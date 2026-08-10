use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

use crate::photon::ChatMessage;
use crate::sniffer::PacketSniffer;
use crate::translator::TranslationEngine;

pub struct AppState {
    pub sniffer: Arc<Mutex<PacketSniffer>>,
    pub translator: Arc<Mutex<TranslationEngine>>,
}

impl AppState {
    pub fn new(tx: mpsc::Sender<ChatMessage>) -> Self {
        Self {
            sniffer: Arc::new(Mutex::new(PacketSniffer::new(tx))),
            translator: Arc::new(Mutex::new(TranslationEngine::new())),
        }
    }
}
