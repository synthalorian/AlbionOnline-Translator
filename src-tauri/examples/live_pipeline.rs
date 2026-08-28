use albion_translator_lib::photon;
use albion_translator_lib::sniffer::PacketSniffer;
use tokio::sync::mpsc;

/// Headless end-to-end pipeline check: capture -> photon decode -> translate -> print.
/// Run: sudo -E cargo run --release --example live_pipeline  (or setcap the example binary)
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("albion_translator_lib=debug,info")
        .init();

    let (tx, mut rx) = mpsc::channel::<photon::ChatMessage>(100);
    let mut sniffer = PacketSniffer::new(tx);
    sniffer.start(None).expect("capture start");

    // Heartbeat: proves whether OTHER tasks get scheduled while the blocking
    // pcap next_packet() loop is running.
    tokio::spawn(async {
        let mut i = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            i += 1;
            eprintln!("HEARTBEAT {}", i);
        }
    });

    println!("PIPELINE LIVE — waiting for chat messages (ctrl-c to quit)");
    let mut n = 0;
    while let Some(msg) = rx.recv().await {
        n += 1;
        println!(
            "[{}] {} | {} | {:?} => {:?}",
            n, msg.channel, msg.sender, msg.text, msg.translated_text
        );
        if n >= 10 {
            break;
        }
    }
    sniffer.stop();
}
