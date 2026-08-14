//! decode_probe — run the app's FULL PhotonDecoder::decode() chain against
//! live traffic (extract_udp_payload + decode). If this rejects everything
//! while the wire probe parses fine, the bug is in the decoder's framing.
use albion_translator_lib::network::extract_udp_payload;
use albion_translator_lib::photon::PhotonDecoder;
use pcap::{Capture, Device};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("albion_translator_lib=debug,info")
        .with_writer(std::io::stderr)
        .init();
    let route_dev = std::process::Command::new("ip")
        .args(["-4", "route", "show", "default"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.split_whitespace()
                .skip_while(|&w| w != "dev")
                .nth(1)
                .map(|w| w.to_string())
        });

    let device = Device::list()
        .expect("device list failed")
        .into_iter()
        .find(|d| Some(&d.name) == route_dev.as_ref())
        .or_else(|| Device::lookup().ok().flatten())
        .expect("no device");
    println!("Device: {}", device.name);

    let mut cap = Capture::from_device(device)
        .expect("open failed")
        .promisc(true)
        .snaplen(65535)
        .timeout(1000)
        .open()
        .expect("capture failed");

    cap.filter("udp port 5055 or udp port 5056 or udp port 4535", true)
        .expect("filter failed");

    let mut decoder = PhotonDecoder::new();
    let mut ok = 0;
    let mut none = 0;
    let mut seen = 0;
    let start = std::time::Instant::now();
    println!("Waiting for Albion packets (12s, max 50)...");
    while seen < 50 && start.elapsed() < std::time::Duration::from_secs(12) {
        match cap.next_packet() {
            Ok(packet) => {
                seen += 1;
                // Parse src/dst ports straight from the raw frame (14B eth + 20B ipv4 hdr)
                let ports = if packet.data.len() >= 14 + 20 + 4 {
                    let sp = u16::from_be_bytes([packet.data[34], packet.data[35]]);
                    let dp = u16::from_be_bytes([packet.data[36], packet.data[37]]);
                    Some((sp, dp))
                } else {
                    None
                };
                if let Some((src, dst, payload)) = extract_udp_payload(packet.data) {
                    match decoder.decode(payload) {
                        Some(msg) => {
                            ok += 1;
                            println!(
                                "#{} DECODED {:?}:{:?}->{:?}:{:?} chan={:?} sender={:?} text={:?}",
                                seen, src, ports.map(|p| p.0), dst, ports.map(|p| p.1),
                                msg.channel, msg.sender, msg.text
                            );
                        }
                        None => {
                            none += 1;
                            println!(
                                "#{} NONE {:?}:{:?}->{:?}:{:?} len={} head={:02x?}",
                                seen, src, ports.map(|p| p.0), dst, ports.map(|p| p.1),
                                payload.len(),
                                &payload[..payload.len().min(24)]
                            );
                        }
                    }
                } else {
                    println!("#{} NOEXTRACT {:?} len={}", seen, ports, packet.data.len());
                }
            }
            Err(_) => {}
        }
    }
    println!("Done. packets={} decoded={} rejected={}", seen, ok, none);
}
