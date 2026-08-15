pub mod network;
pub mod sniffer;
pub mod photon;
pub mod translator;
pub mod state;
pub mod license;
pub mod hosts;

use license::LicenseStatus;
use state::AppState;

use tauri::{Emitter, Manager, State};
use tokio::sync::mpsc;
use tracing::info;

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

#[tauri::command]
async fn set_channel_mapping(
    channel_id: i64,
    channel: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let ch = match channel.as_str() {
        "Party" => photon::ChatChannel::Party,
        "Guild" => photon::ChatChannel::Guild,
        "Alliance" => photon::ChatChannel::Alliance,
        "Trade" => photon::ChatChannel::Trade,
        "Global" => photon::ChatChannel::Global,
        "LFG" => photon::ChatChannel::LFG,
        "Recruitment" => photon::ChatChannel::Recruitment,
        "Faction" => photon::ChatChannel::Faction,
        _ => return Err(format!("Unknown channel type: {}", channel)),
    };
    let sniffer = state.sniffer.lock().await;
    sniffer.set_channel_mapping(channel_id, ch);
    Ok(format!("Channel {} mapped to {}", channel_id, channel))
}

#[tauri::command]
async fn download_translation_model(lang: String) -> Result<String, String> {
    let model_dir = dirs::cache_dir()
        .ok_or("No cache directory")?
        .join("albion-translator")
        .join("models");
    std::fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let model_name = format!("opus-mt-{}-en-ct2", lang);
    let model_path = model_dir.join(&model_name);
    if model_path.exists() {
        return Ok(format!("Model {} already downloaded", lang));
    }

    // Download from HuggingFace (pre-converted CTranslate2 models)
    let url = format!(
        "https://huggingface.co/OpenNMT/{}-ct2/resolve/main/model.bin",
        model_name
    );
    info!("Downloading translation model: {} from {}", model_name, url);

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    std::fs::create_dir_all(&model_path).map_err(|e| e.to_string())?;
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    std::fs::write(model_path.join("model.bin"), &bytes).map_err(|e| e.to_string())?;

    // Also download the tokenizer/config files
    for file in &["config.json", "shared_vocabulary.txt", "source.spm", "target.spm"] {
        let file_url = format!(
            "https://huggingface.co/OpenNMT/{}-ct2/resolve/main/{}",
            model_name, file
        );
        if let Ok(resp) = client.get(&file_url).send().await {
            if resp.status().is_success() {
                if let Ok(data) = resp.bytes().await {
                    std::fs::write(model_path.join(file), &data).ok();
                }
            }
        }
    }

    Ok(format!("Downloaded {} model — restart app to activate", lang))
}

#[tauri::command]
async fn check_for_updates(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_updater::UpdaterExt;
    match app.updater().map_err(|e| e.to_string())?.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            info!("Update available: {}", version);
            // Download and install with progress
            match update.download_and_install(|_chunk, _total| {}, || {}).await {
                Ok(()) => Ok(format!("Updated to {} — restart to apply", version)),
                Err(e) => Err(format!("Update download failed: {}", e)),
            }
        }
        Ok(None) => Ok("Already up to date".to_string()),
        Err(e) => Err(format!("Update check failed: {}", e)),
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
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            set_channel_mapping,
            download_translation_model,
            check_for_updates,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
