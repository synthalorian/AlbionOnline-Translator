pub mod sniffer;
pub mod photon;
pub mod translator;
pub mod state;

use sniffer::PacketSniffer;
use state::AppState;
use translator::TranslationEngine;

use tauri::{Emitter, Manager, State};
use tokio::sync::mpsc;

#[tauri::command]
async fn start_capture(state: State<'_, AppState>) -> Result<String, String> {
    let mut sniffer = state.sniffer.lock().await;
    match sniffer.start() {
        Ok(_) => Ok("Capture started".to_string()),
        Err(e) => Err(format!("Failed to start capture: {}", e)),
    }
}

#[tauri::command]
async fn stop_capture(state: State<'_, AppState>) -> Result<String, String> {
    let mut sniffer = state.sniffer.lock().await;
    sniffer.stop();
    Ok("Capture stopped".to_string())
}

#[tauri::command]
async fn get_capture_status(state: State<'_, AppState>) -> Result<bool, String> {
    let sniffer = state.sniffer.lock().await;
    Ok(sniffer.is_running())
}

#[tauri::command]
async fn set_target_language(
    lang: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut engine = state.translator.lock().await;
    engine.set_target_language(&lang);
    Ok(format!("Target language set to {}", lang))
}

#[tauri::command]
async fn get_target_language(state: State<'_, AppState>) -> Result<String, String> {
    let engine = state.translator.lock().await;
    Ok(engine.target_language().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter("albion_translator=debug,info")
        .init();

    let (tx, mut rx) = mpsc::channel::<photon::ChatMessage>(100);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            
            // Spawn the chat message forwarder
            tauri::async_runtime::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    let _ = app_handle.emit("chat-message", &msg);
                }
            });

            let state = AppState::new(tx);
            app.manage(state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_capture,
            stop_capture,
            get_capture_status,
            set_target_language,
            get_target_language,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
