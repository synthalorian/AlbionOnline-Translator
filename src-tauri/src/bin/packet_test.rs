use pcap::{Capture, Device};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use albion_network_lib::{
    DecodedPacket, ExtractedPacket, PhotonParser, PhotonParserConfig,
    extract_udp_payload,
};

fn main() {
    println!("Albion Online Packet Sniffer Test - Both Ports");
    println!("==============================================");
    println!("Capturing UDP ports 5055 AND 5056...");
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

    // Capture BOTH Albion ports
    cap.filter("udp port 5055 or udp port 5056 or udp port 4535", true)
        .expect("Failed to set filter");

    let link_type = cap.get_datalink().0;
    println!("Link type: {}\n", link_type);

    let config = PhotonParserConfig::with_defaults("test".to_string(), false);
    let mut parser = PhotonParser::new(config);
    
    let mut packet_count = 0;
    let mut chat_count = 0;
    let mut event_counts: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
    let mut op_counts: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();

    while running.load(Ordering::SeqCst) {
        match cap.next_packet() {
            Ok(packet) => {
                packet_count += 1;
                
                if let Some(udp_packet) = extract_udp_payload(packet.data, Some(link_type as u16)) {
                    let before = parser.decoded_packets().len();
                    let _ = parser.receive_packet(
                        udp_packet.payload,
                        packet_count,
                        udp_packet.source,
                        udp_packet.destination,
                    );
                    
                    for decoded in &parser.decoded_packets()[before..] {
                        match decoded {
                            DecodedPacket::Event(event) => {
                                *event_counts.entry(event.code as i32).or_insert(0) += 1;
                                
                                if let Some(ExtractedPacket::ChatMessage(msg)) = &event.extracted {
                                    chat_count += 1;
                                    let json = serde_json::to_value(msg).unwrap_or_default();
                                    let player = json.get("player_name").and_then(|v| v.as_str()).unwrap_or("?");
                                    let message = json.get("message").and_then(|v| v.as_str()).unwrap_or("?");
                                    let channel = json.get("channel_type").and_then(|v| v.as_str()).unwrap_or("?");
                                    
                                    println!("[CHAT] [{}] {}: {}", channel, player, message);
                                }
                            }
                            DecodedPacket::Operation(op) => {
                                *op_counts.entry(op.code as i32).or_insert(0) += 1;
                                
                                if op.code as i32 == 189 || op.code as i32 == 193 || op.code as i32 == 194 {
                                    println!("[CHAT OP] Code {:?} detected!", op.code);
                                }
                            }
                            DecodedPacket::Unknown(_) => {}
                        }
                    }
                }
                
                if packet_count % 100 == 0 {
                    println!("Packets: {} | Chat: {} | Events: {:?} | Ops: {:?}", 
                             packet_count, chat_count, event_counts, op_counts);
                }
            }
            Err(_) => {}
        }
    }

    println!("\nTotal packets: {}", packet_count);
    println!("Chat messages: {}", chat_count);
    println!("Event codes: {:?}", event_counts);
    println!("Operation codes: {:?}", op_counts);
}
