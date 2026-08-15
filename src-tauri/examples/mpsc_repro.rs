use tokio::sync::mpsc;

/// Minimal repro: blocking sync loop + mpsc send in one task, recv in another.
/// Mirrors the sniffer's capture-task/translation-worker structure exactly.
#[tokio::main]
async fn main() {
    let (raw_tx, mut raw_rx) = mpsc::channel::<i32>(64);
    let (ui_tx, mut ui_rx) = mpsc::channel::<i32>(100);

    // "translation worker"
    tokio::spawn(async move {
        eprintln!("worker: entering recv loop");
        while let Some(v) = raw_rx.recv().await {
            eprintln!("worker: received {}", v);
            if ui_tx.send(v).await.is_err() {
                break;
            }
        }
        eprintln!("worker: exited");
    });

    // "capture loop" — blocking sync call in the same task as the send
    tokio::spawn(async move {
        let mut i = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2)); // stand-in for cap.next_packet()
            i += 1;
            eprintln!("capture: sending {}", i);
            if raw_tx.send(i).await.is_err() {
                eprintln!("capture: send failed");
                break;
            }
        }
    });

    // "UI consumer"
    let mut n = 0;
    while let Some(v) = ui_rx.recv().await {
        println!("UI GOT {}", v);
        n += 1;
        if n >= 5 {
            println!("MINIMAL REPRO: PIPELINE WORKS");
            return;
        }
    }
}
