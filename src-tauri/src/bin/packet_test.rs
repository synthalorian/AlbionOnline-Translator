use pcap::{Capture, Device};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const MESSAGE_OPERATION_REQUEST: u8 = 2;
const MESSAGE_OPERATION_RESPONSE: u8 = 3;
const MESSAGE_EVENT: u8 = 4;

// Chat operation codes
const OP_SEND_CHAT_MESSAGE: u8 = 189;
const OP_SEND_WHISPER_MESSAGE: u8 = 193;
const OP_SAY: u8 = 194;

// Chat event codes
const EVENT_CHAT_MESSAGE: u8 = 73;
const EVENT_CHAT_SAY: u8 = 74;
const EVENT_CHAT_WHISPER: u8 = 75;

// Photon command types
const COMMAND_SEND_RELIABLE: u8 = 6;
const COMMAND_SEND_UNRELIABLE: u8 = 7;
const COMMAND_SEND_FRAGMENT: u8 = 8;

fn main() {
    println!("Albion Online Packet Sniffer Test - Corrected Protocol");
    println!("======================================================");
    println!("Listening for chat operations and events...");
    println!("Send a chat message NOW!");
    println!("Press Ctrl+C to stop\n");

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    let device = Device::lookup()
        .expect("Failed to lookup device")
        .expect("No device found");

    println!("Using device: {}\n", device.name);

    let mut cap = Capture::from_device(device)
        .expect("Failed to open device")
        .promisc(true)
        .snaplen(65535)
        .timeout(1000)
        .open()
        .expect("Failed to start capture");

    cap.filter("udp port 5056 or udp port 4535", true)
        .expect("Failed to set filter");

    let mut packet_count = 0;
    let mut chat_ops = 0;
    let mut chat_events = 0;

    while running.load(Ordering::SeqCst) {
        match cap.next_packet() {
            Ok(packet) => {
                packet_count += 1;
                let data = packet.data;
                
                if data.len() < 42 {
                    continue;
                }
                
                let payload = &data[42..];
                
                if payload.len() >= 12 {
                    let flags = payload[2];
                    let cmd_count = payload[3];
                    
                    if flags == 1 {
                        continue;
                    }
                    
                    let mut offset = 12;
                    for _ in 0..cmd_count {
                        if offset + 12 > payload.len() {
                            break;
                        }
                        
                        let cmd_type = payload[offset];
                        // Command length includes the 12-byte header
                        let cmd_total_len = u32::from_be_bytes([
                            payload[offset + 4],
                            payload[offset + 5],
                            payload[offset + 6],
                            payload[offset + 7],
                        ]) as usize;
                        
                        if cmd_total_len < 12 || offset + cmd_total_len > payload.len() {
                            break;
                        }
                        
                        // Payload starts after 12-byte command header
                        let cmd_payload = &payload[offset + 12..offset + cmd_total_len];
                        
                        match cmd_type {
                            COMMAND_SEND_RELIABLE => {
                                if let Some((op_or_event, is_chat)) = parse_message(cmd_payload) {
                                    if is_chat {
                                        chat_ops += 1;
                                    }
                                }
                            }
                            COMMAND_SEND_UNRELIABLE => {
                                // Unreliable has 4-byte sequence number prefix
                                if cmd_payload.len() >= 4 {
                                    let msg_payload = &cmd_payload[4..];
                                    if let Some((op_or_event, is_chat)) = parse_message(msg_payload) {
                                        if is_chat {
                                            chat_events += 1;
                                        }
                                    }
                                }
                            }
                            COMMAND_SEND_FRAGMENT => {
                                // Fragment has 20-byte header
                                if cmd_payload.len() >= 20 {
                                    let msg_payload = &cmd_payload[20..];
                                    if let Some((op_or_event, is_chat)) = parse_message(msg_payload) {
                                        if is_chat {
                                            chat_events += 1;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        
                        offset += cmd_total_len;
                    }
                }
                
                if packet_count % 1000 == 0 {
                    println!("Packets: {} | Chat ops: {} | Chat events: {}", 
                             packet_count, chat_ops, chat_events);
                }
            }
            Err(_) => {}
        }
    }

    println!("\n\nCapture stopped.");
    println!("Total packets: {}", packet_count);
    println!("Chat operations: {}", chat_ops);
    println!("Chat events: {}", chat_events);
}

fn parse_message(data: &[u8]) -> Option<(u8, bool)> {
    if data.len() < 2 {
        return None;
    }
    
    // Photon message: first byte is unknown, second byte is message type
    let msg_type = data[1];
    
    match msg_type {
        MESSAGE_OPERATION_REQUEST => {
            if data.len() >= 3 {
                let op_code = data[2];
                let is_chat = matches!(op_code, OP_SEND_CHAT_MESSAGE | OP_SEND_WHISPER_MESSAGE | OP_SAY);
                if is_chat {
                    println!("[OUTGOING CHAT] Op {} detected!", op_code);
                    println!("  Data: {:02x?}", &data[..std::cmp::min(60, data.len())]);
                }
                return Some((op_code, is_chat));
            }
        }
        MESSAGE_EVENT => {
            if data.len() >= 3 {
                let event_code = data[2];
                let is_chat = matches!(event_code, EVENT_CHAT_MESSAGE | EVENT_CHAT_SAY | EVENT_CHAT_WHISPER);
                if is_chat {
                    println!("[INCOMING CHAT] Event {} detected!", event_code);
                    println!("  Data: {:02x?}", &data[..std::cmp::min(60, data.len())]);
                }
                return Some((event_code, is_chat));
            }
        }
        _ => {}
    }
    
    None
}
