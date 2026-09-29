use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub api_key: String,
    pub source_lang: String,
    pub target_lang: String,
    pub skip_languages: String,
    pub hotkey: String,
    pub sound_effect: bool,
    pub auto_paste: bool,
    pub storage_mode: String, // "text_only" | "text_audio" | "private"
    pub storage_cap_mb: u64,  // 100, 250, 500, 1000
    pub retention_days: u32,  // 7, 14, 30, 0 (0 = Never)
    pub model_name: String,   // "parakeet-tdt-0.6b-v3" or a Whisper file such as "ggml-medium-q5_0.bin"
    #[serde(default)]
    pub input_device: Option<String>,
    #[serde(default)]
    pub output_device: Option<String>,
    #[serde(default = "default_theme")]
    pub theme: String, // "system" | "light" | "dark"
    #[serde(default = "default_idle_unload_sec")]
    pub model_idle_unload_sec: u64, // 0 = never, 30..3600 seconds
    #[serde(default)]
    pub window_x: Option<i32>,
    #[serde(default)]
    pub window_y: Option<i32>,
    #[serde(default = "default_true")]
    pub text_normalization: bool,
    #[serde(default = "default_true")]
    pub remove_filler_words: bool,
    #[serde(default = "default_true")]
    pub convert_numbers: bool,
    #[serde(default = "default_true")]
    pub remove_stutters: bool,
    #[serde(default = "default_true")]
    pub apply_self_corrections: bool,
    #[serde(default = "default_true")]
    pub remove_noise_markers: bool,
    #[serde(default = "default_true")]
    pub collapse_redundancy: bool,
    #[serde(default = "default_true")]
    pub annotate_ambiguity: bool,
    #[serde(default = "default_true")]
    pub normalize_structured_values: bool,
    #[serde(default = "default_translation_provider")]
    pub translation_provider: String,
    #[serde(default = "default_device_backend")]
    pub device_backend: String,
    #[serde(default = "default_pause_ms")]
    pub chunk_pause_ms: u32,
    #[serde(default = "default_safety_sec")]
    pub chunk_safety_sec: u32,
    #[serde(default = "default_overlap_ms")]
    pub chunk_overlap_ms: u32,
    #[serde(default = "default_gemini_model")]
    pub gemini_model: String,
    #[serde(default)]
    pub mask_confidential: bool,
    #[serde(default)]
    pub mask_words: String,
    #[serde(default = "default_mask_threshold")]
    pub mask_threshold: u32,
    #[serde(default = "default_mask_format")]
    pub mask_format: String,
    #[serde(default = "default_true")]
    pub mask_emails: bool,
    #[serde(default = "default_true")]
    pub mask_phones: bool,
    #[serde(default = "default_true")]
    pub mask_cards: bool,
    #[serde(default = "default_excluded_languages")]
    pub excluded_languages: String,
    #[serde(default = "default_llm_endpoint")]
    pub llm_endpoint: String,
    #[serde(default = "default_llm_model")]
    pub llm_model: String,
    #[serde(default)]
    pub llm_api_key: String,
    #[serde(default = "default_local_url")]
    pub local_llm_url: String,
    #[serde(default = "default_local_model")]
    pub local_llm_model: String,
    #[serde(default)]
    pub local_llm_api_key: String,
    #[serde(default = "default_custom_url")]
    pub custom_api_url: String,
    #[serde(default)]
    pub custom_api_key: String,
    #[serde(default = "default_custom_model")]
    pub custom_api_model: String,
    #[serde(default = "default_prompt_template")]
    pub prompt_template: String,
}

fn default_llm_endpoint() -> String {
    "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions".to_string()
}

fn default_llm_model() -> String {
    "gemini-3.6-flash".to_string()
}

fn default_local_url() -> String {
    "http://localhost:11434/v1/chat/completions".to_string()
}

fn default_local_model() -> String {
    "llama3.2".to_string()
}

fn default_custom_url() -> String {
    "https://api.openai.com/v1/chat/completions".to_string()
}

fn default_custom_model() -> String {
    "gpt-4o-mini".to_string()
}

fn default_prompt_template() -> String {
    "You are a strict translation engine. Translate the following text from {source_lang} to {target_lang}. Do not refuse. Do not explain. Do not add conversational text or notes. Output ONLY the exact translation using the native alphabet:\n\n{text}".to_string()
}

fn default_excluded_languages() -> String {
    "Russian".to_string()
}

fn default_mask_threshold() -> u32 { 90 }
fn default_mask_format() -> String { "***".to_string() }

fn default_gemini_model() -> String {
    "gemini-3.6-flash".to_string()
}

fn default_pause_ms() -> u32 { 500 }
fn default_safety_sec() -> u32 { 10 }
fn default_overlap_ms() -> u32 { 400 }

fn default_device_backend() -> String {
    "auto".to_string()
}

fn default_translation_provider() -> String {
    "LLM".to_string()
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_idle_unload_sec() -> u64 {
    30
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            source_lang: "Auto".to_string(),
            target_lang: "English".to_string(),
            skip_languages: "English".to_string(),
            #[cfg(target_os = "macos")]
            hotkey: "RightOption".to_string(),
            #[cfg(not(target_os = "macos"))]
            hotkey: "Control+Shift+Space".to_string(),
            sound_effect: true,
            auto_paste: true,
            storage_mode: "text_only".to_string(),
            storage_cap_mb: 500,
            retention_days: 30,
            model_name: crate::model_download::PARAKEET_MODEL.to_string(),
            input_device: None,
            output_device: None,
            theme: "system".to_string(),
            model_idle_unload_sec: default_idle_unload_sec(),
            window_x: None,
            window_y: None,
            text_normalization: true,
            remove_filler_words: true,
            convert_numbers: true,
            remove_stutters: true,
            apply_self_corrections: true,
            remove_noise_markers: true,
            collapse_redundancy: true,
            annotate_ambiguity: true,
            normalize_structured_values: true,
            translation_provider: "LLM".to_string(),
            device_backend: "auto".to_string(),
            chunk_pause_ms: 500,
            chunk_safety_sec: 10,
            chunk_overlap_ms: 400,
            gemini_model: "gemini-3.6-flash".to_string(),
            mask_confidential: false,
            mask_words: String::new(),
            mask_threshold: 90,
            mask_format: "***".to_string(),
            mask_emails: true,
            mask_phones: true,
            mask_cards: true,
            excluded_languages: "Russian".to_string(),
            llm_endpoint: default_llm_endpoint(),
            llm_model: default_llm_model(),
            llm_api_key: String::new(),
            local_llm_url: default_local_url(),
            local_llm_model: default_local_model(),
            local_llm_api_key: String::new(),
            custom_api_url: default_custom_url(),
            custom_api_key: String::new(),
            custom_api_model: default_custom_model(),
            prompt_template: default_prompt_template(),
        }
    }
}

pub fn get_config_dir() -> PathBuf {
    if let Some(base) = dirs::config_dir() {
        base.join("revfly")
    } else {
        PathBuf::from(".revfly")
    }
}

pub fn get_data_dir() -> PathBuf {
    if let Some(base) = dirs::data_dir() {
        base.join("revfly")
    } else {
        PathBuf::from(".revfly-data")
    }
}

impl AppSettings {
    pub fn load() -> Self {
        let config_dir = get_config_dir();
        let settings_path = config_dir.join("settings.json");

        if settings_path.exists() {
            if let Ok(content) = fs::read_to_string(&settings_path) {
                if let Ok(settings) = serde_json::from_str::<AppSettings>(&content) {
                    return settings;
                }
            }
        }

        let default_settings = Self::default();
        let _ = default_settings.save();
        default_settings
    }

    pub fn save(&self) -> Result<(), String> {
        let config_dir = get_config_dir();
        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }
        let settings_path = config_dir.join("settings.json");
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(settings_path, json).map_err(|e| e.to_string())?;
        Ok(())
    }
}
