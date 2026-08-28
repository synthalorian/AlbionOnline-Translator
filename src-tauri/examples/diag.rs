use albion_translator_lib::sniffer;

fn main() {
    let report = sniffer::run_diagnostic(None, 3, false).expect("diagnostic");
    println!("device: {}", report.device);
    println!(
        "total={} udp={} albion={}",
        report.total_packets, report.udp_packets, report.albion_packets
    );
    println!("top ports: {:?}", report.top_udp_ports);
}
