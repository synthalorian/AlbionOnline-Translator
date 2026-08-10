use pcap::{Capture, Device};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use albion_network_lib::{
    DecodedPacket, ExtractedPacket, PhotonParser, PhotonParserConfig,
    extract_udp_payload,
};

fn main() {
    println!("Albion Online Packet Sniffer Test - albion-network-lib");
    println!("======================================================");
    println!("Listening for chat messages...");
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

    let config = PhotonParserConfig::with_defaults("test".to_string(), false);
    let mut parser = PhotonParser::new(config);
    
    let mut packet_count = 0;
    let mut chat_count = 0;

    while running.load(Ordering::SeqCst) {
        match cap.next_packet() {
            Ok(packet) => {
                packet_count += 1;
                
                if let Some(udp_packet) = extract_udp_payload(packet.data, None) {
                    let before = parser.decoded_packets().len();
                    let _ = parser.receive_packet(
                        udp_packet.payload,
                        packet_count,
                        udp_packet.source,
                        udp_packet.destination,
                    );
                    
                    for decoded in &parser.decoded_packets()[before..] {
                        if let DecodedPacket::Event(event) = decoded {
                            if let Some(ExtractedPacket::ChatMessage(msg)) = &event.extracted {
                                chat_count += 1;
                                let json = serde_json::to_value(msg).unwrap_or_default();
                                let player = json.get("player_name").and_then(|v| v.as_str()).unwrap_or("?");
                                let message = json.get("message").and_then(|v| v.as_str()).unwrap_or("?");
                                let channel = json.get("channel_type").and_then(|v| v.as_str()).unwrap_or("?");
                                
                                println!("[CHAT] [{}] {}: {}", channel, player, message);
                            }
                        }
                    }
                }
                
                if packet_count % 1000 == 0 {
                    println!("Packets: {} | Chat messages: {}", packet_count, chat_count);
                }
            }
            Err(_) => {}
        }
    }

    println!("\n\nCapture stopped.");
    println!("Total packets: {}", packet_count);
    println!("Chat messages: {}", chat_count);
}
