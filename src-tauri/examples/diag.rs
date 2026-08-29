use albion_translator_lib::sniffer;

fn main() {
    let report = sniffer::run_diagnostic(None, 12, false).expect("diagnostic");
    println!("device: {}", report.device);
    println!(
        "total={} udp={} albion={} (in {} / out {})",
        report.total_packets,
        report.udp_packets,
        report.albion_packets,
        report.albion_inbound,
        report.albion_outbound
    );
    println!(
        "photon: encrypted={} chat_decoded={}",
        report.albion_encrypted, report.photon_chat_decoded
    );
    println!("top ports: {:?}", report.top_udp_ports);
}
