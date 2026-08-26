pub mod network;
pub mod sniffer;
pub mod photon;
pub mod translator;
pub mod state;
pub mod hosts;

use state::AppState;

use tauri::{Emitter, Manager, State};
use tokio::sync::mpsc;
use tracing::info;

#[tauri::command]
async fn start_capture(state: State<'_, AppState>) -> Result<String, String> {
    let mut sniffer = state.sniffer.lock().await;
    match sniffer.start() {
        Ok(device) => Ok(format!("Capture started on {}", device)),
        Err(e) => {
            let mut msg = format!("Failed to start capture: {}", e);
            // The bundled wpcap.dll/Packet.dll only let the app START — actual
            // capture needs the Npcap driver/service from the full install.
            #[cfg(target_os = "windows")]
            {
                msg.push_str(
                    "\n\nCapture on Windows requires the Npcap driver. Install it from https://npcap.com (default options), then retry.",
                );
            }
            Err(msg)
        }
    }
}

/// Capture diagnostics: (running, packets captured this session). The UI
/// polls this while capturing — 0 packets means we're on the wrong interface,
/// >0 with no chat means the decoder/filter is dropping things.
#[tauri::command]
async fn get_capture_stats(state: State<'_, AppState>) -> Result<(bool, u64), String> {
    let sniffer = state.sniffer.lock().await;
    Ok((sniffer.is_running(), sniffer.packet_count()))
}

/// All capturable network devices with addresses — diagnostic aid for
/// wrong-interface captures.
#[tauri::command]
async fn list_capture_devices() -> Result<Vec<String>, String> {
    Ok(crate::sniffer::list_devices())
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

#[tauri::command]
async fn set_click_through(enabled: bool, app: tauri::AppHandle) -> Result<String, String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_ignore_cursor_events(enabled)
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "Click-through {}",
            if enabled { "enabled" } else { "disabled" }
        ))
    } else {
        Err("Main window not found".to_string())
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
            app.manage(state);

            let app_handle = app.handle().clone();

            // Spawn the chat message forwarder
            tauri::async_runtime::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    let _ = app_handle.emit("chat-message", &msg);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_capture,
            stop_capture,
            get_capture_status,
            get_capture_stats,
            list_capture_devices,
            set_target_language,
            get_target_language,
            translate_user_text,
            set_channel_mapping,
            download_translation_model,
            check_for_updates,
            set_click_through,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
