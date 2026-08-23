use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info, warn};
use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};
use rusqlite::Connection;
use std::sync::Mutex as StdMutex;

/// Above this lingua confidence, text is assumed to already be the target
/// language and translation is skipped. Below it, text goes to Google sl=auto.
const SKIP_TARGET_CONFIDENCE: f64 = 0.75;

/// Minimum lingua confidence before trusting a detected language enough to
/// route text through that language's local CT2 model.
const CT2_MIN_CONFIDENCE: f64 = 0.5;

/// Translation engine with pluggable backends
/// Priority: CTranslate2 (local) > Google Translate (free, no key) > fallback tag

/// Albion-specific glossary — terms that Google Translate mangles.
/// Applied as pre-translation replacement so the API sees consistent English.
static GLOSSARY: &[(&str, &str)] = &[
    ("HO", "hideout"),
    ("ZvZ", "zerg versus zerg"),
    ("Gank", "gank"),
    ("Ganking", "ganking"),
    ("LFG", "looking for group"),
    ("LFM", "looking for members"),
    ("WTS", "want to sell"),
    ("WTB", "want to buy"),
    ("WTT", "want to trade"),
    ("PVE", "PvE"),
    ("PVP", "PvP"),
    ("AO", "Albion Online"),
    ("BZ", "black zone"),
    ("RZ", "red zone"),
    ("YZ", "yellow zone"),
    ("CTA", "call to arms"),
    ("Zerg", "zerg"),
    ("Ava", "Avalonian"),
    ("Avas", "Avalonians"),
    ("Dive", "dive"),
    ("Diving", "diving"),
    ("Static", "static dungeon"),
    ("Estatica", "static dungeon"),
    ("Roaming", "roaming"),
    ("Gank", "gank"),
    ("Blob", "blob"),
    ("Clap", "clap"),
    ("Bomb", "bomb squad"),
    ("Bombsquad", "bomb squad"),
    ("Rat", "rat"),
    ("Rats", "rats"),
    ("Fame", "fame"),
    ("Spec", "specialization"),
    ("Specs", "specializations"),
    ("IP", "item power"),
    ("MP", "masterpiece"),
    ("T8", "tier 8"),
    ("T7", "tier 7"),
    ("T6", "tier 6"),
    ("T5", "tier 5"),
    ("T4", "tier 4"),
    ("8.3", "8.3"),
    ("8.4", "8.4"),
    ("Recluta", "recruiting"),
    ("Reclutando", "recruiting"),
    ("Reclutamiento", "recruitment"),
    ("Busco", "looking for"),
    ("Procuro", "looking for"),
    ("Grupales", "group dungeons"),
    ("Dorados", "gold chests"),
    ("Azules", "blue chests"),
    ("Caminos", "roads"),
    ("Gremio", "guild"),
    ("Guilda", "guild"),
];
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
    db: Option<StdMutex<Connection>>,
    google_api_key: Option<String>,
    http_client: reqwest::Client,
    detector: LanguageDetector,
    model_dir: PathBuf,
    /// CTranslate2 translators keyed by source language code (e.g. "es", "pt").
    /// Each model translates FROM that language TO the target (currently always English).
    ct2_translators: HashMap<String, ct2rs::Translator<ct2rs::tokenizers::auto::Tokenizer>>,
}

impl TranslationEngine {
    pub fn new() -> Self {
        let google_api_key = std::env::var("GOOGLE_TRANSLATE_API_KEY").ok();

        // Build lingua detector with common Albion languages.
        // NOTE: this list only gates the LOCAL fast paths (skip-if-target and
        // CT2 model pick). Anything uncertain falls through to Google sl=auto,
        // which detects 100+ languages server-side.
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
            Language::Polish,
            Language::Ukrainian,
            Language::Italian,
            Language::Vietnamese,
            Language::Indonesian,
        ];

        let detector = LanguageDetectorBuilder::from_languages(&languages)
            // 0.25 was too strict for slangy game chat ("Busco players con
            // experiencia en el gankeo" came back unknown). 0.1 catches
            // mixed game-speak; lingua still returns None on true noise.
            .with_minimum_relative_distance(0.1)
            .build();

        // Look for CTranslate2 models
        let model_dir = Self::find_model_dir();
        let ct2_translators = Self::load_ct2_models(&model_dir);

        Self {
            target_language: "en".to_string(),
            cache: HashMap::new(),
            db: Self::init_cache_db().map(StdMutex::new),
            google_api_key,
            // Bounded timeout: the translation worker is a single sequential
            // loop — one hung request would silently kill translation for the
            // whole session. 5s is generous for the gtx endpoint.
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            detector,
            model_dir,
            ct2_translators,
        }
    }

    /// Initialize SQLite translation cache at ~/.cache/albion-translator/cache.db.
    /// In-memory HashMap is the hot path; SQLite is the persistent backing store.
    fn init_cache_db() -> Option<Connection> {
        let cache_dir = dirs::cache_dir()?
            .join("albion-translator");
        std::fs::create_dir_all(&cache_dir).ok()?;
        let db_path = cache_dir.join("cache.db");
        let conn = Connection::open(&db_path).ok()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS translations (
                key TEXT PRIMARY KEY,
                translated TEXT NOT NULL,
                created_at INTEGER DEFAULT (unixepoch())
            );
            CREATE INDEX IF NOT EXISTS idx_created ON translations(created_at);"
        ).ok()?;
        info!("Translation cache DB opened at {:?}", db_path);
        Some(conn)
    }

    /// Look up a translation in the cache (memory first, then SQLite).
    fn cache_get(&self, key: &str) -> Option<String> {
        // Hot path: in-memory
        if let Some(cached) = self.cache.get(key) {
            return Some(cached.clone());
        }
        // Cold path: SQLite
        if let Some(ref db) = self.db {
            if let Ok(db) = db.lock() {
                let result: Result<String, _> = db.query_row(
                    "SELECT translated FROM translations WHERE key = ?1",
                    [key],
                    |row| row.get(0),
                );
                if let Ok(translated) = result {
                    return Some(Self::apply_glossary(&translated));
                }
            }
        }
        None
    }

    /// Store a translation in both memory and SQLite.
    fn cache_insert(&mut self, key: String, translated: String) {
        self.cache.insert(key.clone(), translated.clone());
        if let Some(ref db) = self.db {
            if let Ok(db) = db.lock() {
                db.execute(
                    "INSERT OR REPLACE INTO translations (key, translated) VALUES (?1, ?2)",
                    [&key, &translated],
                ).ok();
            }
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

    /// Load all available CTranslate2 models from the model directory.
    /// Each model is named "opus-mt-{src}-en-ct2" (e.g. opus-mt-es-en-ct2).
    /// Returns a map of source language code → translator.
    fn load_ct2_models(model_dir: &PathBuf) -> HashMap<String, ct2rs::Translator<ct2rs::tokenizers::auto::Tokenizer>> {
        let mut translators = HashMap::new();
        let supported = ["es", "pt", "ru", "zh", "ko", "ja", "de", "fr", "tr", "ar"];
        for src in &supported {
            let model_path = model_dir.join(format!("opus-mt-{}-en-ct2", src));
            if !model_path.exists() {
                continue;
            }
            let config = ct2rs::Config::default();
            match ct2rs::Translator::new(&model_path, &config) {
                Ok(translator) => {
                    info!("Loaded CTranslate2 model: {} → en", src);
                    translators.insert(src.to_string(), translator);
                }
                Err(e) => {
                    warn!("Failed to load CTranslate2 model {}: {}", src, e);
                }
            }
        }
        if translators.is_empty() {
            info!("No CTranslate2 models found in {:?}, local translation disabled", model_dir);
        } else {
            info!("Loaded {} CTranslate2 models", translators.len());
        }
        translators
    }

    pub fn set_target_language(&mut self, lang: &str) {
        self.target_language = lang.to_string();
        info!("Target language set to: {}", lang);
    }

    pub fn target_language(&self) -> &str {
        &self.target_language
    }

    /// Check if local translation is available for any language
    pub fn has_local_translation(&self) -> bool {
        !self.ct2_translators.is_empty()
    }

    /// Apply the Albion glossary to translated text — ensures game terms
    /// are consistent in English output (e.g. "hiding place" → "hideout",
    /// "looking for group" stays as "LFG").
    fn apply_glossary(text: &str) -> String {
        let mut result = text.to_string();
        for (term, replacement) in GLOSSARY {
            // Case-insensitive whole-word replacement
            let lower = result.to_lowercase();
            let term_lower = term.to_lowercase();
            if let Some(pos) = lower.find(&term_lower) {
                let before = &lower[..pos];
                let after = &lower[pos + term_lower.len()..];
                // Only replace if it's a whole word (not part of another word)
                let word_before = before.chars().last().map(|c| !c.is_alphanumeric()).unwrap_or(true);
                let word_after = after.chars().next().map(|c| !c.is_alphanumeric()).unwrap_or(true);
                if word_before && word_after {
                    result = format!("{}{}{}", &result[..pos], replacement, &result[pos + term.len()..]);
                }
            }
        }
        result
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

        // Confidence-gated skip — see translate() for rationale.
        if self.language_confidence(trimmed, target_lang) >= SKIP_TARGET_CONFIDENCE {
            return None;
        }

        let cache_key = format!(
            "{}:{}:{}",
            detected.as_deref().unwrap_or("auto"),
            target_lang,
            trimmed
        );

        if let Some(cached) = self.cache_get(&cache_key) {
            debug!("Cache hit for: {}", trimmed);
            return Some(cached.clone());
        }

        // Try CTranslate2 if we have a model for the detected source language
        // (confidence-gated — wrong-model garbage gets cached otherwise).
        if target_lang == "en" {
            if let Some(ref det) = detected {
                if self.language_confidence(trimmed, det) >= CT2_MIN_CONFIDENCE {
                    if let Some(translator) = self.ct2_translators.get(det.as_str()) {
                        match self.translate_ct2(translator, trimmed, Some(det)).await {
                            Ok(translated) => {
                                self.cache_insert(cache_key, translated.clone());
                                return Some(Self::apply_glossary(&translated));
                            }
                            Err(e) => {
                                debug!("CTranslate2 failed: {}", e);
                            }
                        }
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
                self.cache_insert(cache_key, translated.clone());
                return Some(Self::apply_glossary(&translated));
            }
            Err(e) => {
                debug!("Google Translate (free) failed: {}", e);
            }
        }

        // Fallback: return original with language tag.
        // NEVER cache this — caching an untranslated fallback poisons the
        // cache permanently (burned 2026-08-22: Google 429s got cached and
        // kept serving untranslated text after service recovered).
        let lang_tag = detected.unwrap_or_else(|| "unknown".to_string());
        let fallback = format!("[{}] {}", lang_tag, trimmed);
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

        // Skip ONLY when lingua is CONFIDENT the text is already the target
        // language. Best-guess equality is not enough: with relative distance
        // 0.1 lingua always picks something, and short/slangy/unsupported chat
        // (Polish, Ukrainian, mixed game-speak) was getting misclassified as
        // English and silently dropped. Uncertain text goes to Google sl=auto,
        // which detects 100+ languages server-side; Google returns identity for
        // true target-language text and that gets cached, so repeat English
        // spam costs nothing after the first sighting.
        if self.language_confidence(trimmed, &self.target_language.clone()) >= SKIP_TARGET_CONFIDENCE {
            return None;
        }

        let cache_key = format!(
            "{}:{}:{}",
            detected.as_deref().unwrap_or("auto"),
            self.target_language,
            trimmed
        );

        if let Some(cached) = self.cache_get(&cache_key) {
            debug!("Cache hit for: {}", trimmed);
            return Some(cached.clone());
        }

        // Try CTranslate2 if we have a model for the detected source language —
        // but only when detection is reasonably sure. Ramming e.g. Polish
        // through the German model produces garbage that then gets CACHED.
        if let Some(ref det) = detected {
            if self.language_confidence(trimmed, det) >= CT2_MIN_CONFIDENCE {
                if let Some(translator) = self.ct2_translators.get(det.as_str()) {
                    match self.translate_ct2(translator, trimmed, Some(det)).await {
                        Ok(translated) => {
                            self.cache_insert(cache_key, translated.clone());
                            return Some(Self::apply_glossary(&translated));
                        }
                        Err(e) => {
                            debug!("CTranslate2 failed: {}", e);
                        }
                    }
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
                self.cache_insert(cache_key, translated.clone());
                return Some(Self::apply_glossary(&translated));
            }
            Err(e) => {
                debug!("Google Translate (free) failed: {}", e);
            }
        }

        // Fallback: return original with language tag.
        // NEVER cache this — caching an untranslated fallback poisons the
        // cache permanently (burned 2026-08-22: Google 429s got cached and
        // kept serving untranslated text after service recovered).
        let lang_tag = detected.unwrap_or_else(|| "unknown".to_string());
        let fallback = format!("[{}] {}", lang_tag, trimmed);
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

    /// Map a lingua Language to our ISO-ish code. Shared by detect_language
    /// and language_confidence so the two never drift apart.
    fn language_to_code(lang: Language) -> &'static str {
        match lang {
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
            Language::Polish => "pl",
            Language::Ukrainian => "uk",
            Language::Italian => "it",
            Language::Vietnamese => "vi",
            Language::Indonesian => "id",
            _ => "unknown",
        }
    }

    /// lingua's confidence (0.0–1.0) that `text` is the given language code.
    /// lingua reports only its top 5 candidates; anything outside that set
    /// is treated as 0.0 — which correctly routes it to Google sl=auto.
    fn language_confidence(&self, text: &str, lang_code: &str) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        self.detector
            .compute_language_confidence_values(text)
            .iter()
            .find(|(lang, _)| Self::language_to_code(*lang) == lang_code)
            .map(|(_, conf)| *conf)
            .unwrap_or(0.0)
    }

    /// Detect language using lingua-rs (best guess — use language_confidence
    /// for gating decisions; this is for display tags and cache keys only)
    pub fn detect_language(&self, text: &str) -> Option<String> {
        if text.is_empty() || text.len() < 3 {
            return None;
        }

        let detected = self.detector.detect_language_of(text)?;
        Some(Self::language_to_code(detected).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_english_is_confidently_target() {
        let engine = TranslationEngine::new();
        let conf = engine.language_confidence("anyone up for a yellow zone fame farm run tonight", "en");
        assert!(conf >= SKIP_TARGET_CONFIDENCE, "clear English scored {}", conf);
    }

    #[test]
    fn spanish_is_not_skipped_as_english() {
        let engine = TranslationEngine::new();
        let conf = engine.language_confidence("busco grupo para gankear en la zona negra", "en");
        assert!(conf < SKIP_TARGET_CONFIDENCE, "Spanish scored EN {}", conf);
        // es/pt confusion is a known lingua quirk — the display tag may say
        // either, but Google's sl=auto resolves it correctly server-side.
        let det = engine.detect_language("busco grupo para gankear en la zona negra");
        assert!(matches!(det.as_deref(), Some("es") | Some("pt")), "got {:?}", det);
    }

    #[test]
    fn polish_is_detected_not_force_english() {
        let engine = TranslationEngine::new();
        let text = "szukam grupy na zółtą strefę, ktoś chętny";
        let conf = engine.language_confidence(text, "en");
        assert!(conf < SKIP_TARGET_CONFIDENCE, "Polish scored EN {}", conf);
        assert_eq!(engine.detect_language(text).as_deref(), Some("pl"));
    }

    #[test]
    fn slangy_mixed_chat_is_not_confidently_english() {
        let engine = TranslationEngine::new();
        // The class of message that used to be silently dropped.
        let conf = engine.language_confidence("vamos bz t8 lfg", "en");
        assert!(conf < SKIP_TARGET_CONFIDENCE, "slang scored EN {}", conf);
    }

    #[test]
    fn unsupported_language_falls_through_to_google() {
        let engine = TranslationEngine::new();
        // Romanian is NOT in the detector list — must not be confidently EN.
        let conf = engine.language_confidence("caut grup pentru bătăli de facțiuni diseară", "en");
        assert!(conf < SKIP_TARGET_CONFIDENCE, "Romanian scored EN {}", conf);
    }
}
