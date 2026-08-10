use pcap::{Capture, Device};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// Photon message types (from albion-network-lib)
const MESSAGE_OPERATION_REQUEST: u8 = 2;
const MESSAGE_OPERATION_RESPONSE: u8 = 3;
const MESSAGE_EVENT: u8 = 4;
const MESSAGE_ENCRYPTED: u8 = 131;

fn main() {
    println!("Albion Online Packet Sniffer Test - Fixed Protocol");
    println!("==================================================");
    println!("Listening on UDP ports 5056 and 4535...");
    println!("Make sure Albion Online is running and IN A CITY with chat active!");
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
    let mut chat_count = 0;
    let mut event_counts = std::collections::HashMap::new();

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
                        continue; // Skip encrypted
                    }
                    
                    let mut offset = 12;
                    for _ in 0..cmd_count {
                        if offset + 12 > payload.len() {
                            break;
                        }
                        
                        let cmd_type = payload[offset];
                        let cmd_len = u32::from_be_bytes([
                            payload[offset + 4],
                            payload[offset + 5],
                            payload[offset + 6],
                            payload[offset + 7],
                        ]) as usize;
                        
                        if cmd_len < 12 || offset + cmd_len > payload.len() {
                            break;
                        }
                        
                        // Check for SendReliable/SendUnreliable
                        if (cmd_type == 6 || cmd_type == 7) && cmd_len > 14 {
                            // Photon message: first byte is unknown, second byte is message type
                            let msg_type = payload[offset + 13];
                            
                            if msg_type == MESSAGE_EVENT {
                                let event_code = payload[offset + 14];
                                *event_counts.entry(event_code).or_insert(0) += 1;
                                
                                // Chat events: 73=ChatMessage, 74=ChatSay, 75=ChatWhisper
                                if event_code == 73 || event_code == 74 || event_code == 75 {
                                    chat_count += 1;
                                    println!("[CHAT] Event {} detected! (packet #{})", event_code, packet_count);
                                }
                            }
                        }
                        
                        offset += cmd_len;
                    }
                }
                
                if packet_count % 500 == 0 {
                    println!("Packets: {} | Chat: {} | Events: {:?}", 
                             packet_count, chat_count, event_counts);
                }
            }
            Err(_) => {}
        }
    }

    println!("\n\nCapture stopped.");
    println!("Total packets: {}", packet_count);
    println!("Chat messages detected: {}", chat_count);
    println!("Event code distribution: {:?}", event_counts);
}
