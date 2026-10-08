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

        let rss_before = crate::vitals::process_rss_mb();
        let started = Instant::now();
        let parakeet = parakeet_rs::ParakeetTDT::from_pretrained(dir_str, None)
            .map_err(|e| format!("Failed to load Parakeet: {:?}", e))?;

        self.model = Some(parakeet);
        self.current_dir = dir_str.to_string();
        crate::pipeline_logger::log_stage_event(
            &get_data_dir(),
            "MODEL_LOAD",
            &format!(
                "Loaded Parakeet TDT in {} ms; app memory {} → {} MB",
                started.elapsed().as_millis(),
                rss_before,
                crate::vitals::process_rss_mb()
            ),
        );
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
        let mut texts = Vec::new();
        for range in model_segments(samples, 16000) {
            let result = parakeet
                .transcribe_samples(samples[range].to_vec(), 16000, 1, None)
                .map_err(|e| format!("Parakeet transcription error: {:?}", e))?;
            let text = result.text.trim().to_string();
            if !text.is_empty() {
                texts.push(text);
            }
        }
        let inference_ms = t0.elapsed().as_millis() as u64;

        let hw = crate::cpu_features::detect_hardware_profile();
        let thread_count = hw.recommended_threads;

        Ok((texts.join(" "), "auto".to_string(), inference_ms, thread_count))
    }
}

/// Longest audio sent to Parakeet in one pass. The model's position table ends near 4 m 50 s
/// (3651 encoder steps of 80 ms) and attention memory grows with the square of the length.
const MAX_SEGMENT_SEC: usize = 60;
/// A long recording is cut at the quietest 20 ms within this many seconds before the limit.
const CUT_SEARCH_SEC: usize = 10;

/// Splits speech into model-sized pieces, cutting in pauses so no word is split.
fn model_segments(samples: &[f32], sample_rate: usize) -> Vec<std::ops::Range<usize>> {
    let max_len = MAX_SEGMENT_SEC * sample_rate;
    let search = CUT_SEARCH_SEC * sample_rate;
    let frame = sample_rate / 50;
    let mut segments = Vec::new();
    let mut start = 0;
    while samples.len() - start > max_len {
        let window_end = start + max_len;
        let window_start = window_end - search;
        let quietest = (window_start..window_end - frame)
            .step_by(frame)
            .min_by(|&a, &b| frame_energy(&samples[a..a + frame]).total_cmp(&frame_energy(&samples[b..b + frame])))
            .map(|f| f + frame / 2)
            .unwrap_or(window_end);
        segments.push(start..quietest);
        start = quietest;
    }
    if start < samples.len() {
        segments.push(start..samples.len());
    }
    segments
}

fn frame_energy(frame: &[f32]) -> f32 {
    frame.iter().map(|x| x * x).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_audio_is_one_segment() {
        let samples = vec![0.1f32; 16000 * 30];
        assert_eq!(model_segments(&samples, 16000), vec![0..samples.len()]);
    }

    #[test]
    fn long_audio_is_cut_in_pauses_and_fully_covered() {
        // 12 minutes of "speech" with a 100 ms pause every 7 seconds.
        let rate = 16000;
        let samples: Vec<f32> = (0..rate * 720)
            .map(|i| if (i / (rate / 10)) % 70 == 69 { 0.0 } else { 0.2 * ((i as f32) * 0.05).sin() })
            .collect();
        let segments = model_segments(&samples, rate);

        assert!(segments.len() >= 12, "{} segments", segments.len());
        assert_eq!(segments.first().unwrap().start, 0);
        assert_eq!(segments.last().unwrap().end, samples.len());
        for pair in segments.windows(2) {
            assert_eq!(pair[0].end, pair[1].start, "segments must be contiguous");
        }
        for s in &segments {
            assert!(s.len() <= MAX_SEGMENT_SEC * rate);
            if s.end != samples.len() {
                assert_eq!(samples[s.end], 0.0, "cut at {} is not in a pause", s.end);
            }
        }
    }
}
