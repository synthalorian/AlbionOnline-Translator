//! wire_probe — hex-dump raw UDP payloads on Albion's ports.
//! Ground-truth tool: read the wire ourselves, never trust comments.
use pcap::{Capture, Device};

fn main() {
    // Pick the interface that owns the default route (never tailscale0/lo)
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

    let link_type = cap.get_datalink().0;
    println!("Link type: {} — waiting for Albion packets (max 40)...", link_type);

    let mut seen = 0;
    while seen < 40 {
        match cap.next_packet() {
            Ok(packet) => {
                let d = packet.data;
                // Ethernet(14) or SLL(16) → IPv4
                let ip_off = if d.len() > 14 && d[12] == 0x08 && d[13] == 0x00 {
                    14
                } else if d.len() > 16 && d[14] == 0x08 && d[15] == 0x00 {
                    16
                } else {
                    0
                };
                if d.len() < ip_off + 28 {
                    continue;
                }
                let ihl = ((d[ip_off] & 0x0f) as usize) * 4;
                if d[ip_off + 9] != 17 {
                    continue; // not UDP
                }
                let udp_off = ip_off + ihl;
                let src_port = u16::from_be_bytes([d[udp_off], d[udp_off + 1]]);
                let dst_port = u16::from_be_bytes([d[udp_off + 2], d[udp_off + 3]]);
                let udp_len = u16::from_be_bytes([d[udp_off + 4], d[udp_off + 5]]) as usize;
                let payload = &d[udp_off + 8..(udp_off + 8 + udp_len.saturating_sub(8)).min(d.len())];
                if payload.len() < 12 {
                    continue;
                }
                seen += 1;
                let src = format!(
                    "{}.{}.{}.{}:{}",
                    d[ip_off + 12], d[ip_off + 13], d[ip_off + 14], d[ip_off + 15], src_port
                );
                let dst = format!(
                    "{}.{}.{}.{}:{}",
                    d[ip_off + 16], d[ip_off + 17], d[ip_off + 18], d[ip_off + 19], dst_port
                );
                println!(
                    "\n#{} len={} {} -> {}",
                    seen,
                    payload.len(),
                    src,
                    dst
                );
                let n = payload.len().min(96);
                for chunk in payload[..n].chunks(16) {
                    print!("  ");
                    for b in chunk {
                        print!("{:02x} ", b);
                    }
                    // pad short chunks so ascii column aligns
                    for _ in chunk.len()..16 {
                        print!("   ");
                    }
                    print!(" |");
                    for b in chunk {
                        let c = *b as char;
                        print!("{}", if c.is_ascii_graphic() || c == ' ' { c } else { '.' });
                    }
                    println!("|");
                }
            }
            Err(_) => {}
        }
    }
    println!("\nDone.");
}
