use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::trace;

/// Photon Unity Networking protocol decoder for Albion Online
/// Based on reverse engineering from albion-online-addons and albion-translator

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub timestamp: String,
    pub channel: ChatChannel,
    pub sender: String,
    pub text: String,
    pub source_lang: Option<String>,
    pub translated_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChatChannel {
    Say,
    Whisper,
    Party,
    Guild,
    Alliance,
    Global,
    Trade,
    LFG,
    Unknown,
}

impl std::fmt::Display for ChatChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatChannel::Say => write!(f, "Say"),
            ChatChannel::Whisper => write!(f, "Whisper"),
            ChatChannel::Party => write!(f, "Party"),
            ChatChannel::Guild => write!(f, "Guild"),
            ChatChannel::Alliance => write!(f, "Alliance"),
            ChatChannel::Global => write!(f, "Global"),
            ChatChannel::Trade => write!(f, "Trade"),
            ChatChannel::LFG => write!(f, "LFG"),
            ChatChannel::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Photon packet types
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum PhotonPacketType {
    Init = 0,
    InitResponse = 1,
    Operation = 2,
    OperationResponse = 3,
    Event = 4,
    InternalOperation = 6,
    InternalOperationResponse = 7,
    Message = 8,
    RawMessage = 9,
}

/// Photon operation codes for Albion
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum AlbionOperation {
    // Chat operations
    ChatSay = 188,
    ChatWhisper = 189,
    ChatParty = 190,
    ChatGuild = 191,
    ChatAlliance = 192,
    
    // Event codes (server -> client)
    EventChatMessage = 210,
    EventChatMessage2 = 211,
}

#[derive(Clone)]
pub struct PhotonDecoder {
    // Message definition cache
    event_map: HashMap<u8, String>,
}

impl PhotonDecoder {
    pub fn new() -> Self {
        let mut event_map = HashMap::new();
        // Populate from messages.json
        event_map.insert(1, "Leave".to_string());
        event_map.insert(6, "HealthUpdate".to_string());
        event_map.insert(25, "NewCharacter".to_string());
        event_map.insert(73, "UpdateFame".to_string());
        event_map.insert(80, "CharacterEquipmentChanged".to_string());
        event_map.insert(81, "RegenerationHealthChanged".to_string());
        event_map.insert(154, "KnockedDown".to_string());
        event_map.insert(188, "ChatSay".to_string());
        event_map.insert(210, "ChatMessage".to_string());
        event_map.insert(211, "ChatMessage2".to_string());
        event_map.insert(214, "PartyInvitation".to_string());
        event_map.insert(215, "PartyJoined".to_string());
        event_map.insert(216, "PartyDisbanded".to_string());
        event_map.insert(217, "PartyPlayerJoined".to_string());
        event_map.insert(218, "PartyChangedOrder".to_string());
        event_map.insert(219, "PartyPlayerLeft".to_string());
        event_map.insert(220, "PartyLeaderChanged".to_string());
        event_map.insert(221, "PartyLootSettingChangedPlayer".to_string());
        event_map.insert(222, "PartySilverGained".to_string());
        event_map.insert(223, "PartyPlayerUpdated".to_string());
        event_map.insert(224, "PartyInvitationPlayerBusy".to_string());
        event_map.insert(225, "PartyMarkedObjectsUpdated".to_string());
        event_map.insert(226, "PartyOnClusterPartyJoined".to_string());
        event_map.insert(227, "PartySetRoleFlag".to_string());

        Self { event_map }
    }

    /// Decode a Photon UDP packet
    /// Returns Some(ChatMessage) if this is a chat message, None otherwise
    pub fn decode(&self, data: &[u8]) -> Option<ChatMessage> {
        if data.len() < 12 {
            return None;
        }

        // Photon header: peer_id (2) + crc_enabled (1) + command_count (1) + timestamp (4) + challenge (4)
        // Then command header: type (1) + channel_id (1) + flags (1) + reserved (1) + length (4)
        
        let mut offset = 12; // Skip Photon header
        
        while offset + 8 <= data.len() {
            let cmd_type = data[offset];
            let _channel_id = data[offset + 1];
            let _flags = data[offset + 2];
            let cmd_length = u32::from_be_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]) as usize;
            
            if cmd_length < 8 || offset + cmd_length > data.len() {
                break;
            }

            let payload = &data[offset + 8..offset + cmd_length];
            
            // Check if this is an event packet
            if cmd_type == PhotonPacketType::Event as u8 {
                if let Some(msg) = self.decode_event(payload) {
                    return Some(msg);
                }
            }
            
            offset += cmd_length;
        }

        None
    }

    fn decode_event(&self, data: &[u8]) -> Option<ChatMessage> {
        if data.len() < 2 {
            return None;
        }

        let event_code = data[0];
        trace!("Event code: {} ({})", event_code, 
               self.event_map.get(&event_code).unwrap_or(&"Unknown".to_string()));

        // Chat message events
        match event_code {
            210 | 211 => self.decode_chat_message(&data[1..]),
            _ => None,
        }
    }

    fn decode_chat_message(&self, data: &[u8]) -> Option<ChatMessage> {
        // Albion chat message structure (reverse engineered):
        // - sender_name: string (2-byte length prefix + UTF-8)
        // - channel_type: byte
        // - message: string (2-byte length prefix + UTF-8)
        
        if data.len() < 3 {
            return None;
        }

        let mut offset = 0;
        
        // Read sender name
        let sender_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        
        if offset + sender_len > data.len() {
            return None;
        }
        
        let sender = String::from_utf8_lossy(&data[offset..offset + sender_len]).to_string();
        offset += sender_len;
        
        if offset >= data.len() {
            return None;
        }
        
        // Read channel type
        let channel_byte = data[offset];
        offset += 1;
        
        let channel = match channel_byte {
            0 => ChatChannel::Say,
            1 => ChatChannel::Whisper,
            2 => ChatChannel::Party,
            3 => ChatChannel::Guild,
            4 => ChatChannel::Alliance,
            5 => ChatChannel::Global,
            6 => ChatChannel::Trade,
            7 => ChatChannel::LFG,
            _ => ChatChannel::Unknown,
        };
        
        // Read message
        if offset + 2 > data.len() {
            return None;
        }
        
        let msg_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;
        
        if offset + msg_len > data.len() {
            return None;
        }
        
        let text = String::from_utf8_lossy(&data[offset..offset + msg_len]).to_string();
        
        // Skip empty messages
        if text.trim().is_empty() {
            return None;
        }

        let now = chrono::Local::now();
        
        Some(ChatMessage {
            timestamp: now.format("%H:%M:%S").to_string(),
            channel,
            sender,
            text,
            source_lang: None,
            translated_text: None,
        })
    }
}

// Add chrono to Cargo.toml dependencies
