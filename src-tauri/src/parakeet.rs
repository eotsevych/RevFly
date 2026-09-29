use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use parakeet_rs::Transcriber;
use tauri::AppHandle;

use crate::settings::get_data_dir;

/// Files needed for the Parakeet TDT 0.6B v3 INT8 model.
/// Source: huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx
const PARAKEET_FILES: &[(&str, u64)] = &[
    ("encoder-model.int8.onnx", 652_183_999),
    ("decoder_joint-model.int8.onnx", 18_202_004),
    ("nemo128.onnx", 139_764),
    ("config.json", 97),
    ("vocab.txt", 93_939),
];

const HF_BASE_URL: &str =
    "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main";

/// Total download size for progress bar (~670 MB).
fn total_model_bytes() -> u64 {
    PARAKEET_FILES.iter().map(|(_, s)| s).sum()
}

/// Wraps the parakeet-rs crate.
/// Caches the loaded model in memory.
pub struct ParakeetTranscriber {
    model: Option<parakeet_rs::ParakeetTDT>,
    current_dir: String,
}

impl ParakeetTranscriber {
    pub fn new() -> Self {
        Self {
            model: None,
            current_dir: String::new(),
        }
    }

    pub fn has_model(&self) -> bool {
        self.model.is_some()
    }

    pub fn unload(&mut self) {
        if self.model.is_some() {
            log::info!("Unloading Parakeet model from RAM");
            self.model = None;
            self.current_dir.clear();
        }
    }

    pub fn is_loaded(&self, model_dir: &Path) -> bool {
        if let Some(s) = model_dir.to_str() {
            self.model.is_some() && self.current_dir == s
        } else {
            false
        }
    }

    pub fn prewarm(&mut self, model_dir: &Path) -> Result<(), String> {
        if !self.is_loaded(model_dir) {
            log::info!("Prewarming Parakeet model into RAM from {:?}", model_dir);
            self.load_model(model_dir)?;
        }
        Ok(())
    }

    /// Path where Parakeet model files are stored.
    pub fn get_model_dir() -> PathBuf {
        let dir = get_data_dir().join("models").join("parakeet-tdt-0.6b-v3");
        if !dir.exists() {
            let _ = fs::create_dir_all(&dir);
        }
        dir
    }

    /// True when all required ONNX files are present on disk.
    pub fn model_ready() -> bool {
        let dir = Self::get_model_dir();
        PARAKEET_FILES
            .iter()
            .all(|(name, _)| dir.join(name).exists())
    }

    /// Total size of downloaded model files in bytes.
    pub fn downloaded_size() -> u64 {
        let dir = Self::get_model_dir();
        PARAKEET_FILES
            .iter()
            .map(|(name, _)| {
                dir.join(name)
                    .metadata()
                    .map(|m| m.len())
                    .unwrap_or(0)
            })
            .sum()
    }

    /// Download all model files from HuggingFace. Skips files already on disk.
    pub async fn ensure_model(app_handle: &AppHandle) -> Result<PathBuf, String> {
        if Self::model_ready() {
            return Ok(Self::get_model_dir());
        }
        // Another caller may already be downloading it; wait, then re-check the disk.
        let _download = crate::model_download::lock().await;
        if Self::model_ready() {
            return Ok(Self::get_model_dir());
        }

        let model = crate::model_download::PARAKEET_MODEL;
        crate::model_download::began(app_handle, model);
        let result = Self::download_model(app_handle).await;
        crate::model_download::finished(app_handle, model, &result);
        result
    }

    async fn download_model(app_handle: &AppHandle) -> Result<PathBuf, String> {
        let model_dir = Self::get_model_dir();
        let client = reqwest::Client::new();
        let total_size = total_model_bytes();
        let mut cumulative: u64 = 0;

        for (filename, _expected) in PARAKEET_FILES {
            let file_path = model_dir.join(filename);

            // Skip files already downloaded
            if file_path.exists() && file_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                cumulative += file_path.metadata().map(|m| m.len()).unwrap_or(0);
                continue;
            }

            let url = format!("{}/{}", HF_BASE_URL, filename);

            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Download failed for {}: {}", filename, e))?;

            if !resp.status().is_success() {
                return Err(format!("HTTP {} for {}", resp.status(), filename));
            }

            let temp_path = file_path.with_extension("downloading");
            let mut file = fs::File::create(&temp_path)
                .map_err(|e| format!("Cannot create {}: {}", filename, e))?;

            let mut resp = resp;
            while let Some(chunk) = resp
                .chunk()
                .await
                .map_err(|e| format!("Chunk error for {}: {}", filename, e))?
            {
                file.write_all(&chunk)
                    .map_err(|e| format!("Write error: {}", e))?;
                cumulative += chunk.len() as u64;

                let percent = (cumulative as f64 / total_size as f64 * 100.0) as u32;
                crate::model_download::progress(
                    app_handle,
                    crate::model_download::PARAKEET_MODEL,
                    percent,
                    cumulative,
                    total_size,
                );
            }

            file.flush().map_err(|e| e.to_string())?;
            // Close the file first: Windows refuses to rename a file that is still open.
            drop(file);
            fs::rename(&temp_path, &file_path)
                .map_err(|e| format!("Rename failed for {}: {}", filename, e))?;
        }

        Ok(model_dir)
    }

    /// Load the ONNX model into memory.
    fn load_model(&mut self, model_dir: &Path) -> Result<(), String> {
        let dir_str = model_dir
            .to_str()
            .ok_or_else(|| "Bad model path".to_string())?;

        let parakeet = parakeet_rs::ParakeetTDT::from_pretrained(dir_str, None)
            .map_err(|e| format!("Failed to load Parakeet: {:?}", e))?;

        self.model = Some(parakeet);
        self.current_dir = dir_str.to_string();
        Ok(())
    }

    /// Transcribe 16 kHz f32 mono samples.
    /// Returns (text, detected_lang, inference_ms, thread_count).
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        model_dir: &Path,
    ) -> Result<(String, String, u64, i32), String> {
        if samples.is_empty() {
            return Ok((String::new(), "auto".to_string(), 0, 0));
        }

        let dir_str = model_dir.to_str().unwrap_or_default();
        if self.model.is_none() || self.current_dir != dir_str {
            self.load_model(model_dir)?;
        }

        let parakeet = self
            .model
            .as_mut()
            .ok_or("Parakeet model not loaded")?;

        let t0 = Instant::now();
        let result = parakeet
            .transcribe_samples(samples.to_vec(), 16000, 1, None)
            .map_err(|e| format!("Parakeet transcription error: {:?}", e))?;
        let inference_ms = t0.elapsed().as_millis() as u64;

        let hw = crate::cpu_features::detect_hardware_profile();
        let thread_count = hw.recommended_threads;

        Ok((
            result.text.trim().to_string(),
            "auto".to_string(),
            inference_ms,
            thread_count,
        ))
    }
}
