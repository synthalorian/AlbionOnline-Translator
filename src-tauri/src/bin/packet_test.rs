use pcap::{Capture, Device};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const MESSAGE_OPERATION_RESPONSE: u8 = 3;
const MESSAGE_EVENT: u8 = 4;

fn main() {
    println!("Albion Online Packet Sniffer Test - Events + Operations");
    println!("=======================================================");
    println!("Listening on UDP ports 5056 and 4535...");
    println!("TYPE SOMETHING IN CHAT or join a busy city!");
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
    let mut event_counts = std::collections::HashMap::new();
    let mut op_counts = std::collections::HashMap::new();

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
                        let cmd_len = u32::from_be_bytes([
                            payload[offset + 4],
                            payload[offset + 5],
                            payload[offset + 6],
                            payload[offset + 7],
                        ]) as usize;
                        
                        if cmd_len < 12 || offset + cmd_len > payload.len() {
                            break;
                        }
                        
                        if (cmd_type == 6 || cmd_type == 7) && cmd_len > 14 {
                            let msg_type = payload[offset + 13];
                            
                            match msg_type {
                                MESSAGE_EVENT => {
                                    let event_code = payload[offset + 14];
                                    *event_counts.entry(event_code).or_insert(0) += 1;
                                    
                                    if event_code == 73 || event_code == 74 || event_code == 75 {
                                        println!("[CHAT EVENT] Code {} detected!", event_code);
                                    }
                                }
                                MESSAGE_OPERATION_RESPONSE => {
                                    let op_code = payload[offset + 14];
                                    *op_counts.entry(op_code).or_insert(0) += 1;
                                    
                                    // Chat-related operations
                                    if op_code == 188 || op_code == 189 || op_code == 190 {
                                        println!("[CHAT OP] Code {} detected!", op_code);
                                    }
                                }
                                _ => {}
                            }
                        }
                        
                        offset += cmd_len;
                    }
                }
                
                if packet_count % 1000 == 0 {
                    println!("Packets: {} | Events: {:?} | Ops: {:?}", 
                             packet_count, event_counts, op_counts);
                }
            }
            Err(_) => {}
        }
    }

    println!("\n\nCapture stopped.");
    println!("Total packets: {}", packet_count);
    println!("Event codes: {:?}", event_counts);
    println!("Operation codes: {:?}", op_counts);
}
