use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::trace;

/// Photon Unity Networking protocol decoder for Albion Online
/// Based on Protocol18 from albion-network-lib

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
    Faction,
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
            ChatChannel::Faction => write!(f, "Faction"),
            ChatChannel::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Photon decoder with Protocol18 deserialization
#[derive(Clone)]
pub struct PhotonDecoder {
    // Channel state tracking for chat channel mapping
    channel_map: HashMap<i64, ChatChannel>,
}

impl PhotonDecoder {
    pub fn new() -> Self {
        Self {
            channel_map: HashMap::new(),
        }
    }

    /// Decode a Photon UDP packet
    /// Returns Some(ChatMessage) if this is a chat message, None otherwise
    pub fn decode(&self, data: &[u8]) -> Option<ChatMessage> {
        if data.len() < 12 {
            return None;
        }

        // Photon header: peer_id (2) + crc_enabled (1) + command_count (1) + timestamp (4) + challenge (4)
        let flags = data[2];
        let command_count = data[3];
        
        // Skip encrypted packets
        if flags == 1 {
            return None;
        }

        let mut offset = 12;
        
        for _ in 0..command_count {
            if offset + 12 > data.len() {
                break;
            }

            let cmd_type = data[offset];
            let _channel_id = data[offset + 1];
            let _flags = data[offset + 2];
            let cmd_length = u32::from_be_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]) as usize;

            if cmd_length < 12 || offset + cmd_length > data.len() {
                break;
            }

            let payload = &data[offset + 12..offset + cmd_length];
            
            // Check if this is a SendReliable or SendUnreliable command
            if cmd_type == 6 || cmd_type == 7 {
                if let Some(msg) = self.decode_message(payload) {
                    return Some(msg);
                }
            }
            
            offset += cmd_length;
        }

        None
    }

    fn decode_message(&self, data: &[u8]) -> Option<ChatMessage> {
        if data.is_empty() {
            return None;
        }

        // Photon message type
        let msg_type = data[0];
        
        match msg_type {
            2 => self.decode_operation_request(&data[1..]),
            3 => self.decode_operation_response(&data[1..]),
            4 => self.decode_event(&data[1..]),
            _ => None,
        }
    }

    fn decode_operation_request(&self, _data: &[u8]) -> Option<ChatMessage> {
        // Operation requests are client->server, we don't need them for chat display
        None
    }

    fn decode_operation_response(&self, _data: &[u8]) -> Option<ChatMessage> {
        // Operation responses are server->client, we don't need them for chat display
        None
    }

    fn decode_event(&self, data: &[u8]) -> Option<ChatMessage> {
        if data.is_empty() {
            return None;
        }

        let event_code = data[0] as i32;
        trace!("Event code: {}", event_code);

        // Chat events (from albion-network-lib EventCode)
        match event_code {
            73 => self.decode_chat_message(&data[1..]), // ChatMessage
            74 => self.decode_chat_say(&data[1..]),     // ChatSay
            75 => self.decode_chat_whisper(&data[1..]), // ChatWhisper
            _ => None,
        }
    }

    fn decode_chat_message(&self, data: &[u8]) -> Option<ChatMessage> {
        // ChatMessage event structure (Protocol18):
        // param 0: channel_id (compressed i64)
        // param 1: player_name (string)
        // param 2: message (string)
        
        let params = self.deserialize_parameter_table(data)?;
        
        let channel_id = params.get(&0)?.as_i64()?;
        let player_name = params.get(&1)?.as_str()?.to_string();
        let message = params.get(&2)?.as_str()?.to_string();
        
        let channel = self.map_channel(channel_id);
        
        let now = chrono::Local::now();
        
        Some(ChatMessage {
            timestamp: now.format("%H:%M:%S").to_string(),
            channel,
            sender: player_name,
            text: message,
            source_lang: None,
            translated_text: None,
        })
    }

    fn decode_chat_say(&self, data: &[u8]) -> Option<ChatMessage> {
        // ChatSay event structure:
        // param 0: player_name (string)
        // param 1: message (string)
        
        let params = self.deserialize_parameter_table(data)?;
        
        let player_name = params.get(&0)?.as_str()?.to_string();
        let message = params.get(&1)?.as_str()?.to_string();
        
        let now = chrono::Local::now();
        
        Some(ChatMessage {
            timestamp: now.format("%H:%M:%S").to_string(),
            channel: ChatChannel::Say,
            sender: player_name,
            text: message,
            source_lang: None,
            translated_text: None,
        })
    }

    fn decode_chat_whisper(&self, data: &[u8]) -> Option<ChatMessage> {
        // ChatWhisper event structure:
        // param 0: player_name (string)
        // param 1: message (string)
        
        let params = self.deserialize_parameter_table(data)?;
        
        let player_name = params.get(&0)?.as_str()?.to_string();
        let message = params.get(&1)?.as_str()?.to_string();
        
        let now = chrono::Local::now();
        
        Some(ChatMessage {
            timestamp: now.format("%H:%M:%S").to_string(),
            channel: ChatChannel::Whisper,
            sender: player_name,
            text: message,
            source_lang: None,
            translated_text: None,
        })
    }

    fn map_channel(&self, channel_id: i64) -> ChatChannel {
        // From albion-network-lib ChatChannel mapping
        match channel_id {
            0 => ChatChannel::Say,
            3517 => ChatChannel::Guild,
            1868 => ChatChannel::Faction, // Thetford
            1856 => ChatChannel::Faction, // Martlock
            1857 => ChatChannel::Faction, // Bridgewatch
            1858 => ChatChannel::Faction, // Lymhurst
            1859 => ChatChannel::Faction, // Fort Sterling
            1860 => ChatChannel::Faction, // Caerleon
            _ => ChatChannel::Unknown,
        }
    }

    /// Protocol18 parameter table deserialization
    fn deserialize_parameter_table(&self, data: &[u8]) -> Option<HashMap<u8, serde_json::Value>> {
        let mut reader = Reader::new(data);
        let size = reader.read_u8()?;
        
        let mut params = HashMap::new();
        for _ in 0..size {
            let key = reader.read_u8()?;
            let value_type = reader.read_u8()?;
            let value = self.deserialize_value(&mut reader, value_type)?;
            params.insert(key, value);
        }
        
        Some(params)
    }

    fn deserialize_value(&self, reader: &mut Reader, type_code: u8) -> Option<serde_json::Value> {
        match type_code {
            0 | 8 => Some(serde_json::Value::Null),
            2 => Some(serde_json::Value::Bool(reader.read_u8()? != 0)),
            3 => Some(serde_json::json!(reader.read_u8()?)),
            4 => Some(serde_json::json!(reader.read_i16_le()?)),
            5 => Some(serde_json::json!(reader.read_f32_le()?)),
            6 => Some(serde_json::json!(reader.read_f64_le()?)),
            7 => {
                let len = self.read_count(reader)?;
                let bytes = reader.read_bytes(len)?;
                Some(serde_json::Value::String(String::from_utf8_lossy(bytes).to_string()))
            }
            9 => Some(serde_json::json!(self.read_compressed_i32(reader)?)),
            10 => Some(serde_json::json!(self.read_compressed_i64(reader)?)),
            11 => Some(serde_json::json!(reader.read_u8()?)),
            12 => Some(serde_json::json!(-(reader.read_u8()? as i32))),
            13 => Some(serde_json::json!(reader.read_u16_le()?)),
            14 => Some(serde_json::json!(-(reader.read_u16_le()? as i32))),
            15 => Some(serde_json::json!(reader.read_u8()?)),
            16 => Some(serde_json::json!(-(reader.read_u8()? as i64))),
            17 => Some(serde_json::json!(reader.read_u16_le()?)),
            18 => Some(serde_json::json!(-(reader.read_u16_le()? as i64))),
            27 => Some(serde_json::Value::Bool(false)),
            28 => Some(serde_json::Value::Bool(true)),
            _ => None,
        }
    }

    fn read_count(&self, reader: &mut Reader) -> Option<usize> {
        Some(self.read_compressed_u32(reader)? as usize)
    }

    fn read_compressed_u32(&self, reader: &mut Reader) -> Option<u32> {
        let mut value = 0u32;
        let mut shift = 0;
        while shift < 35 {
            let current = reader.read_u8()? as u32;
            value |= (current & 0x7f) << shift;
            if (current & 0x80) == 0 {
                return Some(value);
            }
            shift += 7;
        }
        None
    }

    fn read_compressed_u64(&self, reader: &mut Reader) -> Option<u64> {
        let mut value = 0u64;
        let mut shift = 0;
        while shift < 70 {
            let current = reader.read_u8()? as u64;
            value |= (current & 0x7f) << shift;
            if (current & 0x80) == 0 {
                return Some(value);
            }
            shift += 7;
        }
        None
    }

    fn read_compressed_i32(&self, reader: &mut Reader) -> Option<i32> {
        let value = self.read_compressed_u32(reader)?;
        Some(((value >> 1) as i32) ^ -((value & 1) as i32))
    }

    fn read_compressed_i64(&self, reader: &mut Reader) -> Option<i64> {
        let value = self.read_compressed_u64(reader)?;
        Some(((value >> 1) as i64) ^ -((value & 1) as i64))
    }
}

/// Simple byte reader for Protocol18
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_u8(&mut self) -> Option<u8> {
        if self.pos >= self.data.len() {
            return None;
        }
        let value = self.data[self.pos];
        self.pos += 1;
        Some(value)
    }

    fn read_i16_le(&mut self) -> Option<i16> {
        if self.pos + 2 > self.data.len() {
            return None;
        }
        let value = i16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Some(value)
    }

    fn read_u16_le(&mut self) -> Option<u16> {
        if self.pos + 2 > self.data.len() {
            return None;
        }
        let value = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Some(value)
    }

    fn read_f32_le(&mut self) -> Option<f32> {
        if self.pos + 4 > self.data.len() {
            return None;
        }
        let value = f32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Some(value)
    }

    fn read_f64_le(&mut self) -> Option<f64> {
        if self.pos + 8 > self.data.len() {
            return None;
        }
        let value = f64::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
            self.data[self.pos + 4],
            self.data[self.pos + 5],
            self.data[self.pos + 6],
            self.data[self.pos + 7],
        ]);
        self.pos += 8;
        Some(value)
    }

    fn read_bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        if self.pos + len > self.data.len() {
            return None;
        }
        let bytes = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Some(bytes)
    }
}
