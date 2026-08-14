//! extract_probe — test the app's real extract_udp_payload path against
//! live traffic. Wire probe parses packets itself; this uses the library
//! function the sniffer actually calls, so a failure here is THE bug.
use albion_translator_lib::network::extract_udp_payload;
use pcap::{Capture, Device};

fn main() {
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
    println!("Device: {} (default route: {:?})", device.name, route_dev);

    let mut cap = Capture::from_device(device)
        .expect("open failed")
        .promisc(true)
        .snaplen(65535)
        .timeout(1000)
        .open()
        .expect("capture failed");

    cap.filter("udp port 5055 or udp port 5056 or udp port 4535", true)
        .expect("filter failed");

    println!("Waiting for Albion packets (10s, max 30)...");
    let mut ok = 0;
    let mut none = 0;
    let mut seen = 0;
    let start = std::time::Instant::now();
    while seen < 30 && start.elapsed() < std::time::Duration::from_secs(10) {
        match cap.next_packet() {
            Ok(packet) => {
                seen += 1;
                match extract_udp_payload(packet.data) {
                    Some((src, dst, payload)) => {
                        ok += 1;
                        println!(
                            "#{} OK  len={} {} -> {} payload_bytes={:02x?}...",
                            seen,
                            payload.len(),
                            src,
                            dst,
                            &payload[..payload.len().min(8)]
                        );
                    }
                    None => {
                        none += 1;
                        println!(
                            "#{} NONE len={} first16={:02x?}",
                            seen,
                            packet.data.len(),
                            &packet.data[..packet.data.len().min(16)]
                        );
                    }
                }
            }
            Err(e) => {
                println!("timeout/err: {}", e);
            }
        }
    }
    println!(
        "Done. packets={} extract_ok={} extract_none={}",
        seen, ok, none
    );
}

// --- stage 2: full app chain (extract + PhotonDecoder::decode) ---
// uncomment below; rebuilt via cargo when needed
