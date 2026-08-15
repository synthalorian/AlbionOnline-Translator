use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, trace};

/// Photon Unity Networking protocol decoder for Albion Online
/// Ported from albion-network-lib Protocol18 (commit f373e56)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub timestamp: String,
    pub channel: ChatChannel,
    pub channel_id: i64,
    pub sender: String,
    pub text: String,
    pub source_lang: Option<String>,
    pub translated_text: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ChatChannel {
    Say,
    Whisper,
    Party,
    Guild,
    Alliance,
    Global,
    Trade,
    LFG,
    Recruitment,
    Faction,
    /// Language-specific channels (English, Español, Português, etc.) —
    /// dropped at decode time, never sent to the frontend.
    #[serde(skip)]
    Language,
    Unknown,
}

impl std::fmt::Display for ChatChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatChannel::Say => write!(f, "Local"),
            ChatChannel::Whisper => write!(f, "Whisper"),
            ChatChannel::Party => write!(f, "Party"),
            ChatChannel::Guild => write!(f, "Guild"),
            ChatChannel::Alliance => write!(f, "Alliance"),
            ChatChannel::Global => write!(f, "Global"),
            ChatChannel::Trade => write!(f, "Trade"),
            ChatChannel::LFG => write!(f, "LFG"),
            ChatChannel::Recruitment => write!(f, "Recruitment"),
            ChatChannel::Faction => write!(f, "Faction"),
            ChatChannel::Language => write!(f, "Language"),
            ChatChannel::Unknown => write!(f, "Unknown"),
        }
    }
}

impl ChatChannel {
    /// Maps the JoinedChatChannel (207) param-0 value — a channel-TYPE enum —
    /// to a channel. Table ported from AlbionOnline-Companion's
    /// ChatChannelTracker::MapChatIndex, verified against live captures
    /// 2026-08-11 (typeEnum 8 joined runtime id 2 = Trade, 2 → 18 =
    /// Recruitment, 3 → 19 = LFG). Unknown enums stay Unknown; the caller
    /// falls back to the channel name Albion also sends in param 2.
    fn from_type_enum(type_enum: i64) -> ChatChannel {
        match type_enum {
            // Live-verified 2026-08-15: type enum 1 joins runtime id 17 = Trade
            // (WTS/WTB messages). Companion had this unmapped.
            1 => ChatChannel::Trade,
            2 => ChatChannel::Recruitment, // verified: joined runtime 18
            3 => ChatChannel::LFG,         // verified: joined runtime 19
            5 => ChatChannel::Global,      // verified: joined runtime 21
            7 => ChatChannel::Faction,     // inferred (runtime 22, unverified)
            // Live-verified 2026-08-15: type enum 8 joins runtime id 2, which
            // carries English language chat, NOT Trade. Companion was wrong.
            8 => ChatChannel::Language,
            24 => ChatChannel::Guild,      // high dynamic runtime id
            25 => ChatChannel::Alliance,   // high dynamic runtime id
            26 => ChatChannel::Party,      // inferred by 24/25 sequence
            27 => ChatChannel::Say,        // zone-local, dynamic per cluster
            _ => ChatChannel::Unknown,
        }
    }

    /// Last-resort channel typing from the name string Albion sends in
    /// JoinedChatChannel param 2 (e.g. "LFG", "Faction - Caerleon").
    fn from_channel_name(name: &str) -> ChatChannel {
        let n = name.trim().to_lowercase();
        if n.is_empty() {
            return ChatChannel::Unknown;
        }
        if n.contains("lfg") || n.contains("looking") {
            ChatChannel::LFG
        } else if n.contains("recruit") {
            ChatChannel::Recruitment
        } else if n.contains("trade") {
            ChatChannel::Trade
        } else if n.contains("faction") {
            ChatChannel::Faction
        } else if n.contains("guild") {
            ChatChannel::Guild
        } else if n.contains("alliance") {
            ChatChannel::Alliance
        } else if n.contains("party") || n.contains("group") {
            ChatChannel::Party
        // Language channels — detected by name, dropped at decode time.
        // Covers the major Albion language communities.
        } else if n.contains("english")
            || n.contains("español")
            || n.contains("espanol")
            || n.contains("spanish")
            || n.contains("português")
            || n.contains("portugues")
            || n.contains("portuguese")
            || n.contains("français")
            || n.contains("francais")
            || n.contains("french")
            || n.contains("deutsch")
            || n.contains("german")
            || n.contains("русский")
            || n.contains("russian")
            || n.contains("polski")
            || n.contains("polish")
            || n.contains("türkçe")
            || n.contains("turkish")
            || n.contains("italiano")
            || n.contains("italian")
            || n.contains("日本語")
            || n.contains("japanese")
            || n.contains("한국어")
            || n.contains("korean")
            || n.contains("中文")
            || n.contains("chinese")
            || n.contains("international")
        {
            ChatChannel::Language
        } else if n.contains("global") {
            ChatChannel::Global
        } else if n.contains("say") || n.contains("local") {
            ChatChannel::Say
        } else if n.contains("whisper") {
            ChatChannel::Whisper
        } else {
            ChatChannel::Unknown
        }
    }
}

/// Photon protocol constants (from albion-network-lib photon/command.rs + message.rs)
const COMMAND_DISCONNECT: u8 = 4;
const COMMAND_SEND_RELIABLE: u8 = 6;
const COMMAND_SEND_UNRELIABLE: u8 = 7;
const COMMAND_SEND_FRAGMENT: u8 = 8;

const COMMAND_HEADER_LEN: usize = 12;
const UNRELIABLE_SEQUENCE_LEN: usize = 4;
const FRAGMENT_HEADER_LEN: usize = 20;

const MESSAGE_OPERATION_REQUEST: u8 = 2;
const MESSAGE_OPERATION_RESPONSE: u8 = 3;
const MESSAGE_EVENT: u8 = 4;
const MESSAGE_ENCRYPTED: u8 = 131;

/// Photon decoder with Protocol18 deserialization
#[derive(Clone)]
pub struct PhotonDecoder {
    // Channel state tracking for chat channel mapping. Shared with the
    // sniffer so the UI can inject manual mappings for Unknown channels.
    channel_map: std::sync::Arc<std::sync::Mutex<HashMap<i64, ChatChannel>>>,
    // Reassembles fragmented SendFragment commands
    fragments: FragmentReassembler,
}

impl PhotonDecoder {
    pub fn new() -> Self {
        Self {
            channel_map: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            fragments: FragmentReassembler::new(),
        }
    }

    /// Create a decoder with a shared channel map (for UI-injected mappings).
    pub fn with_channel_map(
        map: std::sync::Arc<std::sync::Mutex<HashMap<i64, ChatChannel>>>,
    ) -> Self {
        Self {
            channel_map: map,
            fragments: FragmentReassembler::new(),
        }
    }

    /// Decode a Photon UDP packet
    /// Returns Some(ChatMessage) if this is a chat message, None otherwise
    pub fn decode(&mut self, data: &[u8]) -> Option<ChatMessage> {
        if data.len() < 12 {
            return None;
        }

        // Photon header: peer_id (2) + flags (1) + command_count (1) + timestamp (4) + challenge (4)
        let flags = data[2];
        let command_count = data[3];

        // Skip encrypted packets
        if flags == 1 {
            return None;
        }

        let mut offset = 12;
        for _ in 0..command_count {
            if data.len().saturating_sub(offset) < COMMAND_HEADER_LEN {
                break;
            }

            let cmd_type = data[offset];
            let raw_length = i32::from_be_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            if raw_length < COMMAND_HEADER_LEN as i32 {
                break;
            }
            let cmd_length = raw_length as usize;
            let payload_offset = offset + COMMAND_HEADER_LEN;
            let next_offset = payload_offset + cmd_length - COMMAND_HEADER_LEN;
            if data.len() < next_offset {
                break;
            }

            let payload = &data[payload_offset..next_offset];

            let message = match cmd_type {
                COMMAND_DISCONNECT => None,
                COMMAND_SEND_RELIABLE => self.decode_message(payload),
                COMMAND_SEND_UNRELIABLE => {
                    if payload.len() < UNRELIABLE_SEQUENCE_LEN {
                        None
                    } else {
                        self.decode_message(&payload[UNRELIABLE_SEQUENCE_LEN..])
                    }
                }
                COMMAND_SEND_FRAGMENT => {
                    if payload.len() < FRAGMENT_HEADER_LEN {
                        None
                    } else {
                        let start_sequence_number =
                            i32::from_be_bytes(payload[0..4].try_into().unwrap());
                        let total_length =
                            i32::from_be_bytes(payload[12..16].try_into().unwrap()) as usize;
                        let fragment_offset =
                            i32::from_be_bytes(payload[16..20].try_into().unwrap()) as usize;

                        match self.fragments.push_fragment(
                            start_sequence_number,
                            total_length,
                            fragment_offset,
                            &payload[FRAGMENT_HEADER_LEN..],
                        ) {
                            Some(complete) => self.decode_message(&complete),
                            None => None,
                        }
                    }
                }
                _ => None,
            };

            if message.is_some() {
                return message;
            }

            offset = next_offset;
        }

        None
    }

    fn decode_message(&mut self, data: &[u8]) -> Option<ChatMessage> {
        if data.len() < 2 {
            return None;
        }

        // Photon message type is at data[1]; data[0] is a leading prefix byte
        let msg_type = data[1];
        let payload = &data[2..];

        match msg_type {
            MESSAGE_OPERATION_REQUEST => self.decode_operation_request(payload),
            MESSAGE_OPERATION_RESPONSE => None, // server->client, not needed for chat display
            MESSAGE_EVENT => self.decode_event(payload),
            MESSAGE_ENCRYPTED => None,
            _ => None,
        }
    }

    /// Outbound (client->server) operations. SendChatMessage (189) is how the
    /// LOCAL player's own chat travels — decoding it is the only way to show
    /// your own messages, since the server doesn't echo them back as events.
    /// Payload structure is not documented in any reference implementation,
    /// so we dump raw params for reverse engineering first.
    fn decode_operation_request(&mut self, data: &[u8]) -> Option<ChatMessage> {
        if data.len() < 2 {
            return None;
        }
        let params = self.deserialize_parameter_table(&mut Reader::new(&data[1..]))?;
        let op_code = value_i64(&params, 253)?;
        // 188=RegisterChatPeer, 189=SendChatMessage, 190=SendModeratorMessage,
        // 191=JoinChatChannel, 192=LeaveChatChannel, 193=SendWhisperMessage,
        // 194=Say (operation_codes.rs). Dumping all of them to learn which op
        // fires on the in-game channel button and what reveals channel types.
        if (188..=194).contains(&op_code) {
            let raw = serde_json::to_string(&params_to_value(
                params.iter().map(|(k, v)| (*k, v.clone())).collect(),
            ))
            .unwrap_or_else(|_| "<unserializable>".to_string());
            info!("Outbound chat op {} raw params: {}", op_code, raw);
        }
        None
    }

    fn decode_event(&mut self, data: &[u8]) -> Option<ChatMessage> {
        if data.is_empty() {
            return None;
        }

        // The first byte of an event payload is a Photon-level code. The real
        // Albion event code rides in parameter 252 (albion-network-lib
        // parse_event_code), except Move (3) which is special-cased from the
        // Photon-level byte. Verified against live wire capture: a chat packet
        // carries payload [0x01][param table: {0: channel, 1: name, 2: text,
        // 252: 73}].
        let photon_event_code = data[0] as i32;
        let mut params = self.deserialize_parameter_table(&mut Reader::new(&data[1..]))?;
        debug!(
            "EVENT photon_code={} params={} raw={:02x?}",
            photon_event_code,
            params.len(),
            &data[..data.len().min(40)]
        );

        if photon_event_code == 3 {
            params.insert(252, serde_json::json!(3)); // Move
        }

        let event_code = parse_event_code(&params)?;
        debug!("Event code: {}", event_code);

        // Chat events (from albion-network-lib EventCode)
        match event_code {
            73 => self.decode_chat_message(&params), // ChatMessage
            74 => self.decode_chat_say(&params),     // ChatSay
            75 => self.decode_chat_whisper(&params), // ChatWhisper
            206 => {
                self.handle_new_chat_channels(&params); // NewChatChannels
                None
            }
            207 => {
                self.handle_joined_chat_channel(&params); // JoinedChatChannel
                None
            }
            208 => {
                self.handle_left_chat_channel(&params); // LeftChatChannel
                None
            }
            _ => None,
        }
    }

    fn handle_new_chat_channels(&mut self, params: &HashMap<u8, serde_json::Value>) {
        // NewChatChannels (206) — the channel ROSTER. Fires at login with every
        // channel you belong to, and again on zone change for the zone-local
        // Say channel. Dump raw params at info level so we can diagnose the
        // wire format from logs.
        let raw = serde_json::to_string(&params_to_value(
            params.iter().map(|(k, v)| (*k, v.clone())).collect(),
        ))
        .unwrap_or_else(|_| "<unserializable>".to_string());
        info!("NewChatChannels raw: {}", raw);

        // Param 0 = channel-TYPE enum. Seen as a little-endian hex string
        // ("1b000000" = 27 = Say) but may also arrive as a plain integer.
        let type_enum = params
            .get(&0)
            .and_then(|v| {
                // Try hex string first
                if let Some(hex) = v.as_str() {
                    let bytes: Vec<u8> = (0..hex.len())
                        .step_by(2)
                        .filter_map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
                        .collect();
                    return match bytes.len() {
                        4 => Some(i64::from(u32::from_le_bytes(bytes.try_into().ok()?))),
                        1 => Some(i64::from(bytes[0])),
                        _ => None,
                    };
                }
                // Try plain integer
                json_value_to_i64(v)
            });
        // Param 1 = array of runtime channel ids. May also be a single id.
        let runtime_ids: Vec<i64> = params
            .get(&1)
            .map(|v| {
                if let Some(arr) = v.as_array() {
                    arr.iter().filter_map(json_value_to_i64).collect()
                } else {
                    json_value_to_i64(v).into_iter().collect()
                }
            })
            .unwrap_or_default();

        let (Some(type_enum), false) = (type_enum, runtime_ids.is_empty()) else {
            info!("NewChatChannels undecoded: {:?}", params);
            return;
        };
        let channel = ChatChannel::from_type_enum(type_enum);
        for id in &runtime_ids {
            info!(
                "Chat channel roster: runtime={} type_enum={} -> {}",
                id, type_enum, channel
            );
            if let Ok(mut map) = self.channel_map.lock() {
                map.insert(*id, channel);
            }
        }
    }

    fn handle_joined_chat_channel(&mut self, params: &HashMap<u8, serde_json::Value>) {
        // JoinedChatChannel (207), semantics verified against live captures via
        // AlbionOnline-Companion's ChatChannelTracker (2026-08-11):
        //   param 0 = channel-TYPE enum (2=Recruitment, 3=LFG, 5=Global,
        //             8=Language(English), 24=Guild, 25=Alliance, 26=Party, 27=Say)
        //   param 1 = RUNTIME channel id — the key ChatMessage events use
        //   param 2 = channel name string ("LFG", "Faction - Caerleon", ...)
        let type_enum = value_i64(params, 0);
        let runtime_id = value_i64(params, 1);
        let (Some(type_enum), Some(runtime_id)) = (type_enum, runtime_id) else {
            debug!("JoinedChatChannel with missing params: {:?}", params);
            return;
        };
        let name = params.get(&2).and_then(|v| v.as_str()).unwrap_or("");
        let mut channel = ChatChannel::from_type_enum(type_enum);
        if channel == ChatChannel::Unknown {
            channel = ChatChannel::from_channel_name(name);
        }
        info!(
            "Chat channel joined: runtime={} type_enum={} name={:?} -> {}",
            runtime_id, type_enum, name, channel
        );
        if let Ok(mut map) = self.channel_map.lock() {
            map.insert(runtime_id, channel);
        }
    }

    fn handle_left_chat_channel(&mut self, params: &HashMap<u8, serde_json::Value>) {
        // LeftChatChannel (208): param 0 = channel_id
        let Some(channel_id) = value_i64(params, 0) else {
            debug!("LeftChatChannel with missing params: {:?}", params);
            return;
        };
        info!("Chat channel left: id={}", channel_id);
        if let Ok(mut map) = self.channel_map.lock() {
            map.remove(&channel_id);
        }
    }

    fn decode_chat_message(&self, params: &HashMap<u8, serde_json::Value>) -> Option<ChatMessage> {
        // ChatMessage event structure (Protocol18):
        // param 0: channel_id
        // param 1: player_name (string)
        // param 2: message (string)

        let channel_id = value_i64(params, 0)?;
        let player_name = params.get(&1)?.as_str()?.to_string();
        let message = params.get(&2)?.as_str()?.to_string();

        let channel = self.map_channel(channel_id);
        // Language channels (English, Español, etc.) are dropped — they don't
        // need translation and just add noise.
        if channel == ChatChannel::Language {
            trace!(
                "ChatMessage dropped (language channel): channel_id={} sender={}",
                channel_id, player_name
            );
            return None;
        }
        // Log unknown high-id channels — likely guild/party/alliance with
        // dynamic runtime ids we haven't mapped yet (missed 206 roster).
        if channel == ChatChannel::Unknown && channel_id > 100 {
            info!(
                "Unmapped channel {}: sender={} text={:?} — \
                 relog with capture running to map guild/party/alliance",
                channel_id, player_name, message
            );
        }
        debug!(
            "ChatMessage: channel_id={} channel={} sender={} text={:?}",
            channel_id, channel, player_name, message
        );

        let now = chrono::Local::now();

        Some(ChatMessage {
            timestamp: now.format("%H:%M:%S").to_string(),
            channel,
            channel_id,
            sender: player_name,
            text: message,
            source_lang: None,
            translated_text: None,
        })
    }

    fn decode_chat_say(&self, params: &HashMap<u8, serde_json::Value>) -> Option<ChatMessage> {
        // ChatSay event structure:
        // param 0: player_name (string)
        // param 1: message (string)

        let player_name = params.get(&0)?.as_str()?.to_string();
        let message = params.get(&1)?.as_str()?.to_string();

        let now = chrono::Local::now();

        Some(ChatMessage {
            timestamp: now.format("%H:%M:%S").to_string(),
            channel: ChatChannel::Say,
            channel_id: 0,
            sender: player_name,
            text: message,
            source_lang: None,
            translated_text: None,
        })
    }

    fn decode_chat_whisper(&self, params: &HashMap<u8, serde_json::Value>) -> Option<ChatMessage> {
        // ChatWhisper event structure:
        // param 0: player_name (string)
        // param 1: message (string)

        let player_name = params.get(&0)?.as_str()?.to_string();
        let message = params.get(&1)?.as_str()?.to_string();

        let now = chrono::Local::now();

        Some(ChatMessage {
            timestamp: now.format("%H:%M:%S").to_string(),
            channel: ChatChannel::Whisper,
            channel_id: -1,
            sender: player_name,
            text: message,
            source_lang: None,
            translated_text: None,
        })
    }

    fn map_channel(&self, channel_id: i64) -> ChatChannel {
        // Resolution order: live map (207 joins) → static well-known ids →
        // Unknown. Static table merges albion-network-lib's faction ids with
        // the companion's capture-verified globals (Trade=2, Recruitment=18,
        // LFG=19, Global=21 — "RECLUTA" spam on 18, "busco party" on 19).
        if let Ok(map) = self.channel_map.lock() {
            if let Some(channel) = map.get(&channel_id) {
                return *channel;
            }
        }
        match channel_id {
            0 => ChatChannel::Say,
            1 => ChatChannel::Global,
            // Live-verified 2026-08-15: id 2 carries the English language
            // channel. Drop it — language channels don't need translation.
            2 => ChatChannel::Language,
            // Live-verified 2026-08-15: id 17 = Trade (WTS/WTB messages,
            // joined via type enum 1).
            17 => ChatChannel::Trade,
            18 => ChatChannel::Recruitment,
            19 => ChatChannel::LFG,
            21 => ChatChannel::Global,
            3517 => ChatChannel::Guild,
            1868 => ChatChannel::Faction, // Thetford
            1856 => ChatChannel::Faction, // Martlock
            1857 => ChatChannel::Faction, // Bridgewatch
            1858 => ChatChannel::Faction, // Lymhurst
            1859 => ChatChannel::Faction, // Fort Sterling
            1860 => ChatChannel::Faction, // Caerleon
            _ => ChatChannel::Unknown, // honest label until a 207 resolves it
        }
    }

    /// Protocol18 parameter table deserialization
    fn deserialize_parameter_table(
        &self,
        reader: &mut Reader,
    ) -> Option<HashMap<u8, serde_json::Value>> {
        let size = reader.read_u8()?;

        let mut params = HashMap::new();
        for _ in 0..size {
            let key = reader.read_u8()?;
            let value_type = reader.read_u8()?;
            let value = self.deserialize(reader, Some(value_type))?;
            params.insert(key, value);
        }

        Some(params)
    }

    fn deserialize(&self, reader: &mut Reader, type_code: Option<u8>) -> Option<serde_json::Value> {
        let type_code = match type_code {
            Some(value) => value,
            None => reader.read_u8()?,
        };

        if (0x80..=228).contains(&type_code) {
            return self.deserialize_custom(reader, type_code);
        }

        match type_code {
            0 | 8 => Some(serde_json::Value::Null),
            2 => Some(serde_json::Value::Bool(reader.read_u8()? != 0)),
            3 => Some(serde_json::json!(reader.read_u8()?)),
            4 => Some(serde_json::json!(reader.read_i16_le()?)),
            5 => Some(serde_json::json!(reader.read_f32_le()?)),
            6 => Some(serde_json::json!(reader.read_f64_le()?)),
            7 => self.read_string(reader).map(serde_json::Value::String),
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
            19 => self.deserialize_custom(reader, 19),
            20 => self.deserialize_dictionary(reader),
            21 => self.deserialize_hashtable(reader),
            23 => self.deserialize_object_array(reader),
            24 => {
                let payload = reader.read_bytes(reader.remaining())?;
                let (code, params) = self.deserialize_operation_request(payload)?;
                Some(serde_json::json!([code, params]))
            }
            25 => {
                let payload = reader.read_bytes(reader.remaining())?;
                let (code, rc, msg, params) = self.deserialize_operation_response(payload)?;
                Some(serde_json::json!([code, rc, msg, params]))
            }
            26 => {
                let payload = reader.read_bytes(reader.remaining())?;
                let (code, params) = self.deserialize_event_data(payload)?;
                Some(serde_json::json!([code, params]))
            }
            27 => Some(serde_json::Value::Bool(false)),
            28 => Some(serde_json::Value::Bool(true)),
            29 | 30 | 31 | 34 => Some(serde_json::json!(0)),
            32 | 33 => Some(serde_json::json!(0.0)),
            0x40 => self.deserialize_array_in_array(reader),
            66 => self.deserialize_boolean_array(reader),
            67 => {
                let len = self.read_count(reader)?;
                let bytes = reader.read_bytes(len)?;
                Some(serde_json::Value::String(hex_lower(bytes)))
            }
            68 => self.read_typed_array(reader, |r| Some(serde_json::json!(r.read_i16_le()?))),
            69 => self.read_typed_array(reader, |r| Some(serde_json::json!(r.read_f32_le()?))),
            70 => self.read_typed_array(reader, |r| Some(serde_json::json!(r.read_f64_le()?))),
            71 => self.read_typed_array(reader, |r| self.read_string(r).map(serde_json::Value::String)),
            73 => self.read_typed_array(reader, |r| {
                Some(serde_json::json!(self.read_compressed_i32(r)?))
            }),
            74 => self.read_typed_array(reader, |r| {
                Some(serde_json::json!(self.read_compressed_i64(r)?))
            }),
            83 => self.deserialize_custom_type_array(reader),
            84 => {
                let (key_type, value_type) = self.deserialize_dictionary_type(reader)?;
                self.read_typed_array(reader, |r| {
                    self.deserialize_dictionary_elements(r, key_type, value_type)
                })
            }
            85 => self.read_typed_array(reader, |r| self.deserialize_hashtable(r)),
            _ => None,
        }
    }

    fn deserialize_operation_request(
        &self,
        payload: &[u8],
    ) -> Option<(u8, serde_json::Value)> {
        let mut reader = Reader::new(payload);
        let operation_code = reader.read_u8()?;
        Some((
            operation_code,
            params_to_value(self.deserialize_parameter_table(&mut reader)?),
        ))
    }

    fn deserialize_operation_response(
        &self,
        payload: &[u8],
    ) -> Option<(u8, i16, String, serde_json::Value)> {
        let mut reader = Reader::new(payload);
        let operation_code = reader.read_u8()?;
        let return_code = reader.read_i16_le()?;
        let mut debug_message = String::new();
        if reader.remaining() > 0 {
            let type_code = reader.read_u8()?;
            if let serde_json::Value::String(value) = self.deserialize(&mut reader, Some(type_code))?
            {
                debug_message = value;
            }
        }
        Some((
            operation_code,
            return_code,
            debug_message,
            params_to_value(self.deserialize_parameter_table(&mut reader)?),
        ))
    }

    fn deserialize_event_data(&self, payload: &[u8]) -> Option<(u8, serde_json::Value)> {
        let mut reader = Reader::new(payload);
        let event_code = reader.read_u8()?;
        Some((
            event_code,
            params_to_value(self.deserialize_parameter_table(&mut reader)?),
        ))
    }

    fn deserialize_dictionary(&self, reader: &mut Reader) -> Option<serde_json::Value> {
        let (key_type, value_type) = self.deserialize_dictionary_type(reader)?;
        self.deserialize_dictionary_elements(reader, key_type, value_type)
    }

    fn deserialize_hashtable(&self, reader: &mut Reader) -> Option<serde_json::Value> {
        let mut map = serde_json::Map::new();
        for _ in 0..self.read_count(reader)? {
            let key = self.deserialize(reader, None)?;
            let value = self.deserialize(reader, None)?;
            if !key.is_null() {
                map.insert(json_key(&key), value);
            }
        }
        Some(serde_json::Value::Object(map))
    }

    fn deserialize_dictionary_type(&self, reader: &mut Reader) -> Option<(u8, u8)> {
        let key_type = reader.read_u8()?;
        let mut value_type = reader.read_u8()?;
        if value_type == 20 {
            let _ = self.deserialize_dictionary_type(reader)?;
        } else if value_type == 0x40 {
            self.consume_dictionary_array_type(reader)?;
            value_type = 0;
        }
        Some((key_type, value_type))
    }

    fn consume_dictionary_array_type(&self, reader: &mut Reader) -> Option<()> {
        let mut type_code = reader.read_u8()?;
        while type_code == 0x40 {
            type_code = reader.read_u8()?;
        }
        Some(())
    }

    fn deserialize_dictionary_elements(
        &self,
        reader: &mut Reader,
        key_type: u8,
        value_type: u8,
    ) -> Option<serde_json::Value> {
        let mut map = serde_json::Map::new();
        for _ in 0..self.read_count(reader)? {
            let key = if key_type == 0 {
                self.deserialize(reader, None)?
            } else {
                self.deserialize(reader, Some(key_type))?
            };
            let value = if value_type == 0 {
                self.deserialize(reader, None)?
            } else {
                self.deserialize(reader, Some(value_type))?
            };
            if !key.is_null() {
                map.insert(json_key(&key), value);
            }
        }
        Some(serde_json::Value::Object(map))
    }

    fn deserialize_object_array(&self, reader: &mut Reader) -> Option<serde_json::Value> {
        self.read_typed_array(reader, |r| self.deserialize(r, None))
    }

    fn deserialize_array_in_array(&self, reader: &mut Reader) -> Option<serde_json::Value> {
        self.read_typed_array(reader, |r| self.deserialize(r, None))
    }

    fn deserialize_boolean_array(&self, reader: &mut Reader) -> Option<serde_json::Value> {
        let len = self.read_count(reader)?;
        let mut result = Vec::with_capacity(len);
        while result.len() < len {
            let value = reader.read_u8()?;
            for bit_index in 0..8 {
                if result.len() >= len {
                    break;
                }
                result.push(serde_json::Value::Bool((value & (1 << bit_index)) != 0));
            }
        }
        Some(serde_json::Value::Array(result))
    }

    fn deserialize_custom_type_array(&self, reader: &mut Reader) -> Option<serde_json::Value> {
        let len = self.read_count(reader)?;
        let type_code = reader.read_u8()?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(self.deserialize_custom_payload(reader, type_code)?);
        }
        Some(serde_json::Value::Array(values))
    }

    fn deserialize_custom(
        &self,
        reader: &mut Reader,
        gp_type: u8,
    ) -> Option<serde_json::Value> {
        let type_code = if gp_type == 19 {
            reader.read_u8()?
        } else {
            gp_type - 0x80
        };
        self.deserialize_custom_payload(reader, type_code)
    }

    fn deserialize_custom_payload(
        &self,
        reader: &mut Reader,
        type_code: u8,
    ) -> Option<serde_json::Value> {
        let len = self.read_count(reader)?;
        Some(serde_json::json!({
            "type_code": type_code,
            "data_hex": hex_lower(reader.read_bytes(len)?)
        }))
    }

    fn read_string(&self, reader: &mut Reader) -> Option<String> {
        let len = self.read_count(reader)?;
        Some(String::from_utf8_lossy(reader.read_bytes(len)?).into_owned())
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

    fn read_typed_array<F>(
        &self,
        reader: &mut Reader,
        mut f: F,
    ) -> Option<serde_json::Value>
    where
        F: FnMut(&mut Reader) -> Option<serde_json::Value>,
    {
        let len = self.read_count(reader)?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(f(reader)?);
        }
        Some(serde_json::Value::Array(values))
    }
}

fn value_i64(params: &HashMap<u8, serde_json::Value>, key: u8) -> Option<i64> {
    params.get(&key).and_then(json_value_to_i64)
}

/// Mirrors albion-network-lib parse_event_code: the real Albion event code is
/// carried in parameter 252, and may be encoded as a signed 16-bit value or as
/// `(code << 4) | 0x01` when the low nibble is a version marker.
fn parse_event_code(params: &HashMap<u8, serde_json::Value>) -> Option<i32> {
    let value = value_i64(params, 252)?;
    let code = to_signed_short(value);
    if is_relevant_event(code) {
        return Some(code);
    }
    let shifted = ((code as i64 & 0xffff) >> 4) as i32;
    if (code & 0x0f) == 0x01 && is_relevant_event(shifted) {
        return Some(shifted);
    }
    None
}

fn to_signed_short(value: i64) -> i32 {
    let mut value = (value & 0xffff) as i32;
    if value >= 0x8000 {
        value -= 0x10000;
    }
    value
}

/// Events the decoder acts on: chat events plus the channel state events that
/// map session-assigned channel_ids to their chat channel (from the AOSnifferNET
/// / albion-network-lib EventCode tables).
fn is_relevant_event(code: i32) -> bool {
    matches!(code, 73 | 74 | 75 | 206 | 207 | 208) // ChatMessage | ChatSay | ChatWhisper | NewChatChannels | JoinedChatChannel | LeftChatChannel
}

fn params_to_value(params: HashMap<u8, serde_json::Value>) -> serde_json::Value {
    serde_json::Value::Object(
        params
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

fn json_value_to_i64(value: &serde_json::Value) -> Option<i64> {
    match value {
        serde_json::Value::Bool(value) => Some(i64::from(*value)),
        serde_json::Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|v| i64::try_from(v).ok())),
        serde_json::Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn json_key(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Bool(v) => v.to_string(),
        serde_json::Value::Number(v) => v.to_string(),
        serde_json::Value::String(v) => v.clone(),
        serde_json::Value::Array(v) => format!("{:?}", v),
        serde_json::Value::Object(_) => "[object]".to_string(),
        serde_json::Value::Null => "null".to_string(),
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

/// Reassembles fragmented Photon commands (COMMAND_SEND_FRAGMENT)
#[derive(Clone, Debug, Default)]
struct FragmentReassembler {
    pending_segments: HashMap<i32, PendingSegment>,
}

#[derive(Clone, Debug)]
struct PendingSegment {
    payload: Vec<u8>,
    written: usize,
    total_length: usize,
}

impl FragmentReassembler {
    fn new() -> Self {
        Self::default()
    }

    /// Push a fragment; returns Some(complete_payload) when all fragments are in
    fn push_fragment(
        &mut self,
        start_sequence_number: i32,
        total_length: usize,
        fragment_offset: usize,
        fragment: &[u8],
    ) -> Option<Vec<u8>> {
        if fragment_offset + fragment.len() > total_length {
            return None;
        }

        let pending = self
            .pending_segments
            .entry(start_sequence_number)
            .or_insert_with(|| PendingSegment {
                payload: vec![0; total_length],
                written: 0,
                total_length,
            });

        if pending.total_length != total_length {
            return None;
        }

        pending.payload[fragment_offset..fragment_offset + fragment.len()]
            .copy_from_slice(fragment);

        pending.written += fragment.len();

        if pending.written >= pending.total_length {
            let payload = self
                .pending_segments
                .remove(&start_sequence_number)
                .expect("pending segment should exist")
                .payload;

            return Some(payload);
        }

        None
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

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_u8(&mut self) -> Option<u8> {
        let byte = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }

    fn read_i16_le(&mut self) -> Option<i16> {
        let bytes: [u8; 2] = self.read_bytes(2)?.try_into().ok()?;
        Some(i16::from_le_bytes(bytes))
    }

    fn read_u16_le(&mut self) -> Option<u16> {
        let bytes: [u8; 2] = self.read_bytes(2)?.try_into().ok()?;
        Some(u16::from_le_bytes(bytes))
    }

    fn read_f32_le(&mut self) -> Option<f32> {
        let bytes: [u8; 4] = self.read_bytes(4)?.try_into().ok()?;
        Some(f32::from_le_bytes(bytes))
    }

    fn read_f64_le(&mut self) -> Option<f64> {
        let bytes: [u8; 8] = self.read_bytes(8)?.try_into().ok()?;
        Some(f64::from_le_bytes(bytes))
    }

    fn read_bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        if self.remaining() < count {
            return None;
        }
        let start = self.pos;
        self.pos += count;
        Some(&self.data[start..self.pos])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reassembles_fragmented_messages() {
        let mut reassembler = FragmentReassembler::new();

        // Message split into two fragments (16 bytes total)
        let fragment1 = vec![1u8; 8];
        let fragment2 = vec![2u8; 8];

        assert!(reassembler
            .push_fragment(100, 16, 0, &fragment1)
            .is_none());
        let complete = reassembler
            .push_fragment(100, 16, 8, &fragment2)
            .expect("should complete");
        assert_eq!(complete.len(), 16);
        assert_eq!(&complete[0..8], &[1u8; 8]);
        assert_eq!(&complete[8..16], &[2u8; 8]);
    }

    #[test]
    fn rejects_out_of_bounds_fragments() {
        let mut reassembler = FragmentReassembler::new();
        assert!(reassembler.push_fragment(1, 10, 8, &[1u8; 4]).is_none());
    }

    /// Build a Photon packet with a single SendUnreliable (type 7) command carrying
    /// a chat event, mirroring the exact wire format observed in live captures:
    ///   [peer_id(2)][flags(1)][cmd_count(1)][timestamp(4)][challenge(4)]
    ///   [cmd_type(1)][channel(1)][cmd_flags(1)][reserved(1)][cmd_len(4)][seq(4)]
    ///   [unreliable_seq(4)] [prefix(1)][msg_type(1)][photon_code(1)][param_table...]
    /// The Albion event code is appended to the param table as key 252 (the real
    /// wire format carries the code there, not in the photon code byte).
    fn build_chat_packet(event_code: u8, params: &[u8]) -> Vec<u8> {
        let mut param_table = vec![params[0] + 1]; // param count + the 252 entry
        param_table.extend_from_slice(&params[1..]);
        param_table.extend_from_slice(&[0xfc, 0x0b, event_code]); // key 252, type u8, code

        let mut msg = vec![0x00]; // prefix byte (skipped by message parser)
        msg.push(0x04); // MESSAGE_EVENT
        msg.push(0x01); // photon-level code (1 for chat events on the wire)
        msg.extend_from_slice(&param_table);

        let mut pkt = Vec::new();
        pkt.extend_from_slice(&[0x00, 0x00]); // peer_id
        pkt.push(0x00); // flags (not encrypted)
        pkt.push(0x01); // command_count
        pkt.extend_from_slice(&[0x5c, 0x5f, 0x3b, 0x7e]); // timestamp
        pkt.extend_from_slice(&[0x27, 0x3f, 0xce, 0x4d]); // challenge
        pkt.push(0x07); // cmd_type = SendUnreliable
        pkt.push(0x00); // channel
        pkt.push(0x00); // cmd_flags
        pkt.push(0x00); // reserved
        // cmd_length placeholder (12-byte header + 4-byte unreliable seq + message)
        let len_offset = pkt.len();
        pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        pkt.extend_from_slice(&[0x00, 0x00, 0x09, 0x36]); // sequence
        pkt.extend_from_slice(&[0x00, 0x00, 0x42, 0x71]); // unreliable sequence
        pkt.extend_from_slice(&msg);

        let cmd_len = 12 + 4 + msg.len();
        pkt[len_offset..len_offset + 4].copy_from_slice(&(cmd_len as u32).to_be_bytes());
        pkt
    }

    #[test]
    fn decodes_chat_message_event() {
        // ChatMessage (73): param 0 = channel_id (compressed i64), 1 = name, 2 = text
        let params = [
            0x03, // param table size
            0x00, 0x0A, 0x00, // key 0, compressed i64, channel_id 0
            0x01, 0x07, 0x04, b'A', b'l', b'b', b'i', // key 1, string "Albi"
            0x02, 0x07, 0x05, b'H', b'e', b'l', b'l', b'o', // key 2, string "Hello"
        ];
        let mut decoder = PhotonDecoder::new();
        let msg = decoder.decode(&build_chat_packet(73, &params)).expect("chat message");

        assert_eq!(msg.sender, "Albi");
        assert_eq!(msg.text, "Hello");
        assert_eq!(msg.channel, ChatChannel::Say);
    }

    #[test]
    fn decodes_chat_say_event() {
        // ChatSay (74): param 0 = name, 1 = text
        let params = [
            0x02, // param table size
            0x00, 0x07, 0x04, b'A', b'l', b'b', b'i', // key 0, string "Albi"
            0x01, 0x07, 0x09, b'H', b'i', b' ', b't', b'h', b'e', b'r', b'e', b'!', // "Hi there!"
        ];
        let mut decoder = PhotonDecoder::new();
        let msg = decoder.decode(&build_chat_packet(74, &params)).expect("chat say");

        assert_eq!(msg.sender, "Albi");
        assert_eq!(msg.text, "Hi there!");
        assert_eq!(msg.channel, ChatChannel::Say);
    }

    #[test]
    fn ignores_non_chat_events() {
        // A non-chat event (e.g. event code 3) must not decode to a chat message.
        let params = [
            0x01, // param table size
            0x00, 0x0A, 0x05, // key 0, compressed i64, value 5
        ];
        let mut decoder = PhotonDecoder::new();
        assert!(decoder.decode(&build_chat_packet(3, &params)).is_none());
    }

    #[test]
    fn resolves_channel_from_join_event() {
        // JoinedChatChannel (207): param 0 = chat_index (u8, 24 = Guild),
        // param 1 = channel_id (compressed i64, 42). After the join, a
        // ChatMessage on channel 42 must resolve to Guild.
        let join_params = [
            0x02, // param table size
            0x00, 0x0B, 24, // key 0, type u8, chat_index 24 (Guild)
            0x01, 0x0A, 0x54, // key 1, compressed i64, channel_id 42
        ];
        let mut decoder = PhotonDecoder::new();
        assert!(decoder.decode(&build_chat_packet(207, &join_params)).is_none());

        let msg_params = [
            0x03, // param table size
            0x00, 0x0A, 0x54, // key 0, compressed i64, channel_id 42
            0x01, 0x07, 0x04, b'A', b'l', b'b', b'i', // key 1, string "Albi"
            0x02, 0x07, 0x05, b'H', b'e', b'l', b'l', b'o', // key 2, string "Hello"
        ];
        let msg = decoder
            .decode(&build_chat_packet(73, &msg_params))
            .expect("chat message after join");
        assert_eq!(msg.channel, ChatChannel::Guild);
    }

    #[test]
    fn leaves_channel_falls_back_to_static_default() {
        // Join then leave: after LeftChatChannel (208) removes channel 42, a
        // message on it falls back to the static default Say (matching the
        // reference lib), since 42 has no hardcoded mapping.
        let join_params = [
            0x02, 0x00, 0x0B, 24, 0x01, 0x0A, 0x54, // index 24 (Guild), id 42
        ];
        let mut decoder = PhotonDecoder::new();
        decoder.decode(&build_chat_packet(207, &join_params));

        let leave_params = [
            0x01, // param table size
            0x00, 0x0A, 0x54, // key 0, compressed i64, channel_id 42
        ];
        decoder.decode(&build_chat_packet(208, &leave_params));

        let msg_params = [
            0x03, 0x00, 0x0A, 0x54, 0x01, 0x07, 0x04, b'A', b'l', b'b', b'i', 0x02, 0x07, 0x05,
            b'H', b'e', b'l', b'l', b'o',
        ];
        let msg = decoder
            .decode(&build_chat_packet(73, &msg_params))
            .expect("chat message after leave");
        // After leaving, runtime id 84 has no live mapping and no static
        // entry — honest Unknown, not a misleading Say.
        assert_eq!(msg.channel, ChatChannel::Unknown);
    }

    #[test]
    fn unknown_type_enum_resolves_to_unknown() {
        // A JoinedChatChannel with an unmapped type enum (e.g. 30) and no
        // channel name param resolves to Unknown.
        let join_params = [
            0x02, 0x00, 0x0B, 30, 0x01, 0x0A, 0x54, // type enum 30 (unmapped), runtime id 84
        ];
        let mut decoder = PhotonDecoder::new();
        decoder.decode(&build_chat_packet(207, &join_params));

        let msg_params = [
            0x03, 0x00, 0x0A, 0x54, 0x01, 0x07, 0x04, b'A', b'l', b'b', b'i', 0x02, 0x07, 0x05,
            b'H', b'e', b'l', b'l', b'o',
        ];
        let msg = decoder
            .decode(&build_chat_packet(73, &msg_params))
            .expect("chat message with unmapped index");
        assert_eq!(msg.channel, ChatChannel::Unknown);
    }

    #[test]
    fn decodes_fragmented_chat_event() {
        // SendFragment (8): [start_seq(4)][??(4)][??(4)][total_len(4)][frag_offset(4)][fragment]
        // Split a chat message into two fragments across two packets.
        let params = [
            0x04, 0x00, 0x0A, 0x00, 0x01, 0x07, 0x04, b'A', b'l', b'b', b'i', 0x02, 0x07, 0x05,
            b'H', b'e', b'l', b'l', b'o', 0xfc, 0x0b, 73,
        ];
        let msg = {
            let mut m = vec![0x00]; // prefix
            m.push(0x04); // MESSAGE_EVENT
            m.push(0x01); // photon-level code
            m.extend_from_slice(&params);
            m
        };
        let (first, second) = msg.split_at(10);

        let mut pkt = |fragment: &[u8], offset: usize| {
            let mut p = Vec::new();
            p.extend_from_slice(&[0x00, 0x00]); // peer_id
            p.push(0x00); // flags
            p.push(0x01); // command_count
            p.extend_from_slice(&[0x5c, 0x5f, 0x3b, 0x7e]); // timestamp
            p.extend_from_slice(&[0x27, 0x3f, 0xce, 0x4d]); // challenge
            p.push(0x08); // cmd_type = SendFragment
            p.push(0x00); // channel
            p.push(0x00); // cmd_flags
            p.push(0x00); // reserved
            let len_offset = p.len();
            p.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // cmd_length placeholder
            p.extend_from_slice(&[0x00, 0x00, 0x00, 0x2A]); // command sequence
            // fragment header: [start_seq(4)][??(4)][??(4)][total_len(4)][frag_offset(4)]
            p.extend_from_slice(&[0x00, 0x00, 0x00, 0x2A]); // start_sequence_number
            p.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // (unused)
            p.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // (unused)
            p.extend_from_slice(&(msg.len() as u32).to_be_bytes()); // total_length
            p.extend_from_slice(&(offset as u32).to_be_bytes()); // fragment_offset
            p.extend_from_slice(fragment);
            let cmd_len = 12 + 20 + fragment.len();
            p[len_offset..len_offset + 4].copy_from_slice(&(cmd_len as u32).to_be_bytes());
            p
        };

        let mut decoder = PhotonDecoder::new();
        // First fragment: not complete yet
        assert!(decoder.decode(&pkt(first, 0)).is_none());
        // Second fragment completes the message
        let decoded = decoder.decode(&pkt(second, first.len())).expect("fragmented chat");
        assert_eq!(decoded.sender, "Albi");
        assert_eq!(decoded.text, "Hello");
    }

    #[test]
    fn static_id_2_is_language_dropped() {
        // Channel id 2 = English language channel (live-verified 2026-08-15).
        // Language channels are dropped at decode time.
        // Zigzag-encoded: 2 → 0x04.
        let msg_params = [
            0x03, 0x00, 0x0A, 0x04, 0x01, 0x07, 0x04, b'A', b'l', b'b', b'i', 0x02, 0x07, 0x05,
            b'H', b'e', b'l', b'l', b'o',
        ];
        let mut decoder = PhotonDecoder::new();
        assert!(decoder.decode(&build_chat_packet(73, &msg_params)).is_none());
    }

    #[test]
    fn language_channel_dropped_via_join_name() {
        // A JoinedChatChannel with an unmapped type enum but a language name
        // ("English") resolves to Language. Messages on that channel are dropped.
        // Zigzag: runtime id 42 → 0x54.
        let join_params = [
            0x03, // param table size
            0x00, 0x0B, 30, // key 0, type u8, type enum 30 (unmapped)
            0x01, 0x0A, 0x54, // key 1, compressed i64, runtime id 42
            0x02, 0x07, 0x07, b'E', b'n', b'g', b'l', b'i', b's', b'h', // key 2, string "English"
        ];
        let mut decoder = PhotonDecoder::new();
        decoder.decode(&build_chat_packet(207, &join_params));

        let msg_params = [
            0x03, 0x00, 0x0A, 0x54, 0x01, 0x07, 0x04, b'A', b'l', b'b', b'i', 0x02, 0x07, 0x05,
            b'H', b'e', b'l', b'l', b'o',
        ];
        // Language channel messages are dropped — decode returns None.
        assert!(decoder.decode(&build_chat_packet(73, &msg_params)).is_none());
    }

    #[test]
    fn language_channel_names_detected() {
        // Various language channel names should resolve to Language.
        assert_eq!(ChatChannel::from_channel_name("English"), ChatChannel::Language);
        assert_eq!(ChatChannel::from_channel_name("Español"), ChatChannel::Language);
        assert_eq!(ChatChannel::from_channel_name("Português"), ChatChannel::Language);
        assert_eq!(ChatChannel::from_channel_name("Français"), ChatChannel::Language);
        assert_eq!(ChatChannel::from_channel_name("Deutsch"), ChatChannel::Language);
        assert_eq!(ChatChannel::from_channel_name("Русский"), ChatChannel::Language);
        assert_eq!(ChatChannel::from_channel_name("International"), ChatChannel::Language);
        // Non-language channels still resolve correctly.
        assert_eq!(ChatChannel::from_channel_name("Trade"), ChatChannel::Trade);
        assert_eq!(ChatChannel::from_channel_name("LFG"), ChatChannel::LFG);
        assert_eq!(ChatChannel::from_channel_name("Global"), ChatChannel::Global);
    }
}
