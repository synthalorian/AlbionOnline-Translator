use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// Translation engine with pluggable backends
/// Supports: Google Translate API, local detection, caching

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
}

impl TranslationEngine {
    pub fn new() -> Self {
        let google_api_key = std::env::var("GOOGLE_TRANSLATE_API_KEY").ok();
        
        Self {
            target_language: "en".to_string(),
            cache: HashMap::new(),
            google_api_key,
            http_client: reqwest::Client::new(),
        }
    }

    pub fn set_target_language(&mut self, lang: &str) {
        self.target_language = lang.to_string();
        info!("Target language set to: {}", lang);
    }

    pub fn target_language(&self) -> &str {
        &self.target_language
    }

    /// Translate text, using cache when possible
    pub async fn translate(&mut self, text: &str, source_lang: Option<&str>) -> Option<String> {
        // Skip translation if text is empty or too short
        let trimmed = text.trim();
        if trimmed.len() < 2 {
            return None;
        }

        // Skip if already in target language (simple heuristic)
        if let Some(detected) = self.detect_language(trimmed) {
            if detected == self.target_language {
                return None;
            }
        }

        let cache_key = format!("{}:{}:{}", 
            source_lang.unwrap_or("auto"), 
            self.target_language,
            trimmed
        );
        
        if let Some(cached) = self.cache.get(&cache_key) {
            debug!("Cache hit for: {}", trimmed);
            return Some(cached.clone());
        }

        // Try Google Translate if API key is available
        if let Some(api_key) = &self.google_api_key {
            match self.translate_google(trimmed, &self.target_language.clone(), api_key).await {
                Ok(translated) => {
                    self.cache.insert(cache_key, translated.clone());
                    return Some(translated);
                }
                Err(e) => {
                    debug!("Google Translate failed: {}", e);
                }
            }
        }

        // Fallback: return original with language tag
        let detected = self.detect_language(trimmed).unwrap_or_else(|| "unknown".to_string());
        let fallback = format!("[{}] {}", detected, trimmed);
        self.cache.insert(cache_key, fallback.clone());
        Some(fallback)
    }

    async fn translate_google(
        &self,
        text: &str,
        target_lang: &str,
        api_key: &str,
    ) -> Result<String, reqwest::Error> {
        let url = format!(
            "https://translation.googleapis.com/language/translate/v2?key={}",
            api_key
        );

        let response = self.http_client
            .post(&url)
            .json(&serde_json::json!({
                "q": text,
                "target": target_lang,
                "format": "text"
            }))
            .send()
            .await?;

        let result: GoogleTranslateResponse = response.json().await?;
        
        Ok(result.data.translations.first()
            .map(|t| t.translated_text.clone())
            .unwrap_or_else(|| text.to_string()))
    }

    /// Detect language of text using character heuristics
    /// TODO: Replace with lingua-rs for proper detection
    pub fn detect_language(&self, text: &str) -> Option<String> {
        if text.is_empty() {
            return None;
        }

        let mut scores: HashMap<&str, usize> = HashMap::new();
        
        for c in text.chars() {
            let code = c as u32;
            
            // Chinese (CJK Unified Ideographs)
            if (0x4E00..=0x9FFF).contains(&code) {
                *scores.entry("zh").or_insert(0) += 2;
            }
            // Japanese (Hiragana, Katakana)
            else if (0x3040..=0x309F).contains(&code) || (0x30A0..=0x30FF).contains(&code) {
                *scores.entry("ja").or_insert(0) += 2;
            }
            // Korean (Hangul)
            else if (0xAC00..=0xD7AF).contains(&code) || (0x1100..=0x11FF).contains(&code) {
                *scores.entry("ko").or_insert(0) += 2;
            }
            // Russian (Cyrillic)
            else if (0x0400..=0x04FF).contains(&code) {
                *scores.entry("ru").or_insert(0) += 2;
            }
            // Arabic
            else if (0x0600..=0x06FF).contains(&code) {
                *scores.entry("ar").or_insert(0) += 2;
            }
            // Thai
            else if (0x0E00..=0x0E7F).contains(&code) {
                *scores.entry("th").or_insert(0) += 2;
            }
            // Portuguese specific
            else if "ãõç".contains(c) {
                *scores.entry("pt").or_insert(0) += 1;
            }
            // Spanish specific
            else if "ñ¿¡".contains(c) {
                *scores.entry("es").or_insert(0) += 1;
            }
            // French specific
            else if "àâäçéèêëîïôöùûüÿ".contains(c) {
                *scores.entry("fr").or_insert(0) += 1;
            }
            // German specific
            else if "äöüß".contains(c) {
                *scores.entry("de").or_insert(0) += 1;
            }
            // Turkish specific
            else if "ğışç".contains(c) {
                *scores.entry("tr").or_insert(0) += 1;
            }
        }

        // Find highest scoring language
        let mut best: Option<(&str, usize)> = None;
        for (lang, score) in &scores {
            if *score >= 2 {
                match best {
                    None => best = Some((lang, *score)),
                    Some((_, best_score)) if *score > best_score => best = Some((lang, *score)),
                    _ => {}
                }
            }
        }

        best.map(|(lang, _)| lang.to_string())
            .or_else(|| Some("en".to_string()))
    }
}
