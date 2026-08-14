pub mod network;
pub mod sniffer;
pub mod photon;
pub mod translator;
pub mod state;
pub mod license;

use license::LicenseStatus;
use state::AppState;

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

// ---------------------------------------------------------------------------
// Licensing
// ---------------------------------------------------------------------------

#[tauri::command]
async fn get_license_status(state: State<'_, AppState>) -> Result<LicenseStatus, String> {
    let mut mgr = state.license.lock().await;
    Ok(mgr.status().await)
}

#[tauri::command]
async fn activate_license(
    key: String,
    state: State<'_, AppState>,
) -> Result<LicenseStatus, String> {
    let mut mgr = state.license.lock().await;
    mgr.activate(&key).await
}

#[tauri::command]
async fn deactivate_license(state: State<'_, AppState>) -> Result<(), String> {
    let mut mgr = state.license.lock().await;
    mgr.deactivate().await
}

#[tauri::command]
async fn get_buy_url() -> String {
    license::BUY_URL.to_string()
}

#[tauri::command]
async fn translate_user_text(
    text: String,
    source_lang: Option<String>,
    target_lang: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let mut engine = state.translator.lock().await;
    let src = source_lang.as_deref();
    match engine.translate_with_target(&text, src, &target_lang).await {
        Some(t) => Ok(t),
        None => Ok(text),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter("albion_translator=debug,albion_translator_lib=debug,info")
        .init();

    let (tx, mut rx) = mpsc::channel::<photon::ChatMessage>(100);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let state = AppState::new(tx);
            let license = state.license.clone();
            app.manage(state);

            let app_handle = app.handle().clone();

            // Spawn the chat message forwarder (license-gated)
            tauri::async_runtime::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    let mut mgr = license.lock().await;
                    if mgr.is_unlocked().await {
                        let _ = app_handle.emit("chat-message", &msg);
                    } else if mgr.take_locked_notice() {
                        let _ = app_handle.emit("license-locked", ());
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_capture,
            stop_capture,
            get_capture_status,
            set_target_language,
            get_target_language,
            get_license_status,
            activate_license,
            deactivate_license,
            get_buy_url,
            translate_user_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
