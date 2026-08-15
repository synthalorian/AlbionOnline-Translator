use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};
use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};

/// Translation engine with pluggable backends
/// Priority: CTranslate2 (local) > Google Translate (free, no key) > fallback tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationRequest {
    pub text: String,
    pub source_lang: Option<String>,
    pub target_lang: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResponse {
    pub translated_text: String,
    pub detected_lang: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoogleTranslateResponse {
    data: GoogleTranslateData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoogleTranslateData {
    translations: Vec<GoogleTranslation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoogleTranslation {
    #[serde(rename = "translatedText")]
    translated_text: String,
    #[serde(rename = "detectedSourceLanguage")]
    detected_source_language: Option<String>,
}

pub struct TranslationEngine {
    target_language: String,
    cache: HashMap<String, String>,
    google_api_key: Option<String>,
    http_client: reqwest::Client,
    detector: LanguageDetector,
    model_dir: PathBuf,
    ct2_translator: Option<ct2rs::Translator<ct2rs::tokenizers::auto::Tokenizer>>,
}

impl TranslationEngine {
    pub fn new() -> Self {
        let google_api_key = std::env::var("GOOGLE_TRANSLATE_API_KEY").ok();

        // Build lingua detector with common Albion languages
        let languages = vec![
            Language::English,
            Language::Spanish,
            Language::Portuguese,
            Language::French,
            Language::German,
            Language::Russian,
            Language::Chinese,
            Language::Japanese,
            Language::Korean,
            Language::Turkish,
            Language::Arabic,
            Language::Thai,
        ];

        let detector = LanguageDetectorBuilder::from_languages(&languages)
            .with_minimum_relative_distance(0.25)
            .build();

        // Look for CTranslate2 models
        let model_dir = Self::find_model_dir();
        let ct2_translator = Self::load_ct2_model(&model_dir);

        Self {
            target_language: "en".to_string(),
            cache: HashMap::new(),
            google_api_key,
            http_client: reqwest::Client::new(),
            detector,
            model_dir,
            ct2_translator,
        }
    }

    fn find_model_dir() -> PathBuf {
        // Check environment variable first
        if let Ok(dir) = std::env::var("ALBION_TRANSLATION_MODEL_DIR") {
            return PathBuf::from(dir);
        }

        // Check next to executable
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let bundled = exe_dir.join("models");
                if bundled.exists() {
                    return bundled;
                }
            }
        }

        // Check user cache
        if let Some(cache_dir) = dirs::cache_dir() {
            let user_models = cache_dir.join("albion-translator").join("models");
            if user_models.exists() {
                return user_models;
            }
        }

        // Default to ./models
        PathBuf::from("models")
    }

    fn load_ct2_model(model_dir: &PathBuf) -> Option<ct2rs::Translator<ct2rs::tokenizers::auto::Tokenizer>> {
        // Try to load es-en model as default
        let model_path = model_dir.join("opus-mt-es-en-ct2");

        if !model_path.exists() {
            info!(
                "No CTranslate2 model found at {:?}, local translation disabled",
                model_path
            );
            return None;
        }

        let config = ct2rs::Config::default();
        match ct2rs::Translator::new(&model_path, &config) {
            Ok(translator) => {
                info!("Loaded CTranslate2 model from {:?}", model_path);
                Some(translator)
            }
            Err(e) => {
                warn!("Failed to load CTranslate2 model: {}", e);
                None
            }
        }
    }

    pub fn set_target_language(&mut self, lang: &str) {
        self.target_language = lang.to_string();
        info!("Target language set to: {}", lang);
    }

    pub fn target_language(&self) -> &str {
        &self.target_language
    }

    /// Check if local translation is available
    pub fn has_local_translation(&self) -> bool {
        self.ct2_translator.is_some()
    }

    /// Translate with an explicit target language (per-call, doesn't mutate engine state).
    /// Used by the user translator iframe for multi-language support.
    pub async fn translate_with_target(
        &mut self,
        text: &str,
        source_lang: Option<&str>,
        target_lang: &str,
    ) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.len() < 2 {
            return None;
        }
        if trimmed.starts_with("http") || trimmed.starts_with("@") {
            return None;
        }

        let detected = source_lang.map(|s| s.to_string())
            .or_else(|| self.detect_language(trimmed));

        // Skip if already in target language
        if let Some(ref det) = detected {
            if det == target_lang {
                return None;
            }
        }

        let cache_key = format!(
            "{}:{}:{}",
            detected.as_deref().unwrap_or("auto"),
            target_lang,
            trimmed
        );

        if let Some(cached) = self.cache.get(&cache_key) {
            debug!("Cache hit for: {}", trimmed);
            return Some(cached.clone());
        }

        // Try CTranslate2 for es→en if model is loaded
        if let Some(ref translator) = self.ct2_translator {
            if target_lang == "en" {
                match self.translate_ct2(translator, trimmed, detected.as_deref()).await {
                    Ok(translated) => {
                        self.cache.insert(cache_key, translated.clone());
                        return Some(translated);
                    }
                    Err(e) => {
                        debug!("CTranslate2 failed: {}", e);
                    }
                }
            }
        }

        // Try free Google Translate (no API key required) — same backend as translate.google.com
        match self.translate_google_free(trimmed, target_lang).await {
            Ok(translated) => {
                info!(
                    "Translated {}->{}: {:?} -> {:?}",
                    detected.as_deref().unwrap_or("auto"),
                    target_lang,
                    trimmed,
                    translated
                );
                self.cache.insert(cache_key, translated.clone());
                return Some(translated);
            }
            Err(e) => {
                debug!("Google Translate (free) failed: {}", e);
            }
        }

        // Fallback: return original with language tag
        let lang_tag = detected.unwrap_or_else(|| "unknown".to_string());
        let fallback = format!("[{}] {}", lang_tag, trimmed);
        self.cache.insert(cache_key, fallback.clone());
        Some(fallback)
    }

    /// Translate text, using cache when possible (uses engine's default target language)
    pub async fn translate(&mut self, text: &str, source_lang: Option<&str>) -> Option<String> {
        // Skip translation if text is empty or too short
        let trimmed = text.trim();
        if trimmed.len() < 2 {
            return None;
        }

        // Skip URLs, mentions, and pure emoji
        if trimmed.starts_with("http") || trimmed.starts_with("@") {
            return None;
        }

        // Detect language if not provided
        let detected = source_lang.map(|s| s.to_string())
            .or_else(|| self.detect_language(trimmed));

        // Skip if already in target language
        if let Some(ref det) = detected {
            if det == &self.target_language {
                return None;
            }
        }

        let cache_key = format!(
            "{}:{}:{}",
            detected.as_deref().unwrap_or("auto"),
            self.target_language,
            trimmed
        );

        if let Some(cached) = self.cache.get(&cache_key) {
            debug!("Cache hit for: {}", trimmed);
            return Some(cached.clone());
        }

        // Try CTranslate2 first (free, local) — currently es→en only
        if let Some(ref translator) = self.ct2_translator {
            match self.translate_ct2(translator, trimmed, detected.as_deref()).await {
                Ok(translated) => {
                    self.cache.insert(cache_key, translated.clone());
                    return Some(translated);
                }
                Err(e) => {
                    debug!("CTranslate2 failed: {}", e);
                }
            }
        }

        // Try free Google Translate (no API key required) — same backend as translate.google.com
        match self.translate_google_free(trimmed, &self.target_language).await {
            Ok(translated) => {
                info!(
                    "Translated {}->{}: {:?} -> {:?}",
                    detected.as_deref().unwrap_or("auto"),
                    self.target_language,
                    trimmed,
                    translated
                );
                self.cache.insert(cache_key, translated.clone());
                return Some(translated);
            }
            Err(e) => {
                debug!("Google Translate (free) failed: {}", e);
            }
        }

        // Fallback: return original with language tag
        let lang_tag = detected.unwrap_or_else(|| "unknown".to_string());
        let fallback = format!("[{}] {}", lang_tag, trimmed);
        self.cache.insert(cache_key, fallback.clone());
        Some(fallback)
    }

    async fn translate_ct2(
        &self,
        translator: &ct2rs::Translator<ct2rs::tokenizers::auto::Tokenizer>,
        text: &str,
        source_lang: Option<&str>,
    ) -> Result<String, anyhow::Error> {
        let _source = source_lang.unwrap_or("es");

        // Use ct2rs translate_batch
        let sources = vec![text.to_string()];
        let options = ct2rs::TranslationOptions::default();

        let results = translator.translate_batch(&sources, &options, None)?;

        let translated = results.first()
            .map(|(text, _)| text.clone())
            .unwrap_or_else(|| text.to_string());

        Ok(translated)
    }

    /// Free Google Translate endpoint — no API key required.
    /// Uses the same backend as translate.google.com (client=gtx, sl=auto).
    /// Response format: [[[sentence_seg, original, null, null, offset], ...], null, "detected_lang", ...]
    /// Multi-sentence: concatenate ALL segments, not just the first.
    async fn translate_google_free(
        &self,
        text: &str,
        target_lang: &str,
    ) -> anyhow::Result<String> {
        let encoded = urlencoding::encode(text);
        let url = format!(
            "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={}&dt=t&q={}",
            target_lang,
            encoded
        );

        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(anyhow::anyhow!("Google Translate returned {}", status));
        }

        // Parse the nested JSON: [[[seg, orig, null, null, off], ...], null, "detected_lang", ...]
        let raw: serde_json::Value = response.json().await?;

        // Concatenate all sentence segments
        let translated = raw
            .get(0)
            .and_then(|sentences| {
                let mut out = String::new();
                if let Some(arr) = sentences.as_array() {
                    for segment in arr {
                        if let Some(seg) = segment.get(0) {
                            if let Some(s) = seg.as_str() {
                                out.push_str(s);
                            }
                        }
                    }
                }
                if out.is_empty() {
                    None
                } else {
                    Some(out)
                }
            })
            .unwrap_or_else(|| text.to_string());

        // Google returns the same text when it couldn't translate or text is already in target
        if translated == text {
            return Ok(text.to_owned());
        }

        Ok(translated)
    }

    /// Detect language using lingua-rs
    pub fn detect_language(&self, text: &str) -> Option<String> {
        if text.is_empty() || text.len() < 3 {
            return None;
        }

        let detected = self.detector.detect_language_of(text)?;

        let code = match detected {
            Language::English => "en",
            Language::Spanish => "es",
            Language::Portuguese => "pt",
            Language::French => "fr",
            Language::German => "de",
            Language::Russian => "ru",
            Language::Chinese => "zh",
            Language::Japanese => "ja",
            Language::Korean => "ko",
            Language::Turkish => "tr",
            Language::Arabic => "ar",
            Language::Thai => "th",
            _ => "unknown",
        };

        Some(code.to_string())
    }
}
