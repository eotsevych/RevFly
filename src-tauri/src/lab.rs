use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::parakeet::ParakeetTranscriber;
use crate::settings::get_data_dir;
use crate::transcribe::{DiagnosticSegment, Transcriber};
use crate::vad::trim_silence;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabAudioItem {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub duration_sec: f32,
    pub size_bytes: u64,
    pub is_vad_trimmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabExperimentRequest {
    pub audio_filename: String,
    pub model_name: String,
    pub use_vad: bool,
    pub language: Option<String>,
    #[serde(default = "lab_default_true")]
    pub text_normalization: bool,
    #[serde(default = "lab_default_true")]
    pub remove_filler_words: bool,
    #[serde(default = "lab_default_true")]
    pub convert_numbers: bool,
    #[serde(default = "lab_default_true")]
    pub remove_stutters: bool,
    #[serde(default = "lab_default_true")]
    pub apply_self_corrections: bool,
    #[serde(default = "lab_default_true")]
    pub remove_noise_markers: bool,
    #[serde(default = "lab_default_true")]
    pub collapse_redundancy: bool,
    #[serde(default = "lab_default_true")]
    pub annotate_ambiguity: bool,
    #[serde(default = "lab_default_true")]
    pub normalize_structured_values: bool,
}

fn lab_default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabExperimentResult {
    pub clean_text: String,
    pub raw_text: String,
    pub raw_output: String,
    pub detected_lang: String,
    pub segments: Vec<DiagnosticSegment>,
    pub original_duration_sec: f32,
    pub processed_duration_sec: f32,
    pub vad_cut_sec: f32,
    pub vad_time_ms: u64,
    pub inference_time_ms: u64,
    pub speed_factor: f32,
    pub hardware_engine: String,
    pub original_audio_filename: String,
    pub vad_audio_filename: Option<String>,
    pub post_applied: Vec<String>,
    pub uncertain_spans: Vec<String>,
    pub requires_clarification: bool,
}

/// Reads WAV file samples into 16kHz float32.
pub fn read_wav_file(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| format!("Failed to open WAV: {}", e))?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(|s| s.ok()).collect(),
        hound::SampleFormat::Int => {
            let max_val = (1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            reader
                .samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
    };

    Ok((samples, sample_rate))
}

/// Lists all audio files available in the app for testing.
pub fn list_available_audio_files() -> Vec<LabAudioItem> {
    let data_dir = get_data_dir();
    let mut list = Vec::new();

    // 1. Check latest_recording.wav (Original Mic Audio)
    let orig_path = data_dir.join("latest_recording.wav");
    if orig_path.exists() {
        if let Ok((samples, rate)) = read_wav_file(&orig_path) {
            let size = orig_path.metadata().map(|m| m.len()).unwrap_or(0);
            list.push(LabAudioItem {
                id: "latest_original".to_string(),
                name: "Latest Mic Recording (Uncut Original)".to_string(),
                filename: "latest_recording.wav".to_string(),
                duration_sec: samples.len() as f32 / rate as f32,
                size_bytes: size,
                is_vad_trimmed: false,
            });
        }
    }

    // 2. Check latest_vad_trimmed.wav (VAD Trimmed Audio)
    let vad_path = data_dir.join("latest_vad_trimmed.wav");
    if vad_path.exists() {
        if let Ok((samples, rate)) = read_wav_file(&vad_path) {
            let size = vad_path.metadata().map(|m| m.len()).unwrap_or(0);
            list.push(LabAudioItem {
                id: "latest_vad".to_string(),
                name: "Latest VAD Trimmed Audio".to_string(),
                filename: "latest_vad_trimmed.wav".to_string(),
                duration_sec: samples.len() as f32 / rate as f32,
                size_bytes: size,
                is_vad_trimmed: true,
            });
        }
    }

    // 3. Check audio/ directory for saved historical recordings
    let audio_dir = data_dir.join("audio");
    if let Ok(entries) = fs::read_dir(&audio_dir) {
        let mut entries_vec: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("wav"))
            .collect();
        entries_vec.sort_by(|a, b| b.cmp(a)); // Newest first

        for p in entries_vec.into_iter().take(10) {
            if let Some(fname) = p.file_name().and_then(|s| s.to_str()) {
                if let Ok((samples, rate)) = read_wav_file(&p) {
                    let size = p.metadata().map(|m| m.len()).unwrap_or(0);
                    list.push(LabAudioItem {
                        id: fname.to_string(),
                        name: format!("History Recording ({})", fname),
                        filename: fname.to_string(),
                        duration_sec: samples.len() as f32 / rate as f32,
                        size_bytes: size,
                        is_vad_trimmed: false,
                    });
                }
            }
        }
    }

    list
}

/// Executes a test experiment on the given audio file.
pub async fn run_experiment(
    app_handle: &AppHandle,
    transcriber: Arc<Mutex<Transcriber>>,
    parakeet_transcriber: Arc<Mutex<ParakeetTranscriber>>,
    req: LabExperimentRequest,
) -> Result<LabExperimentResult, String> {
    let data_dir = get_data_dir();

    // Resolve audio file path
    let audio_path = if req.audio_filename == "latest_recording.wav" || req.audio_filename == "latest" {
        data_dir.join("latest_recording.wav")
    } else if req.audio_filename == "latest_vad_trimmed.wav" {
        data_dir.join("latest_vad_trimmed.wav")
    } else {
        let p = data_dir.join("audio").join(&req.audio_filename);
        if p.exists() {
            p
        } else {
            data_dir.join(&req.audio_filename)
        }
    };

    if !audio_path.exists() {
        return Err(format!("Audio file not found: {:?}", audio_path));
    }

    let (file_samples, sample_rate) = read_wav_file(&audio_path)?;
    // History recordings are saved at the microphone's own rate; the models need 16 kHz.
    let raw_samples = crate::audio::resample_to_16k(&file_samples, sample_rate);
    if raw_samples.is_empty() {
        return Err("Audio file is empty".to_string());
    }

    let original_duration_sec = raw_samples.len() as f32 / 16000.0;

    // Optional VAD stage
    let (processed_samples, vad_time_ms, processed_duration_sec, vad_cut_sec, vad_audio_filename) = if req.use_vad {
        let vad_t0 = Instant::now();
        let vad_res = trim_silence(&raw_samples, 16000);
        let elapsed = vad_t0.elapsed().as_millis() as u64;

        // Save trimmed audio for playback
        let trimmed_path = data_dir.join("lab_vad_trimmed.wav");
        let _ = crate::history::write_wav_file(&trimmed_path, &vad_res.samples, 16000);

        (
            vad_res.samples,
            elapsed,
            vad_res.trimmed_duration_sec,
            vad_res.silence_removed_sec,
            Some("lab_vad_trimmed.wav".to_string()),
        )
    } else {
        (
            raw_samples.clone(),
            0,
            original_duration_sec,
            0.0,
            None,
        )
    };

    let is_parakeet = req.model_name == "parakeet-tdt-0.6b-v3";

    if is_parakeet {
        // Single Model Policy: unload Whisper
        {
            let mut tr = transcriber.lock().unwrap();
            tr.unload();
        }
        let model_dir = ParakeetTranscriber::ensure_model(app_handle).await?;
        let mut pk = parakeet_transcriber.lock().unwrap();
        let t0 = Instant::now();
        let res = pk.transcribe(&processed_samples, &model_dir)?;
        let inference_ms = t0.elapsed().as_millis() as u64;

        let speed_factor = if inference_ms > 0 {
            (processed_duration_sec * 1000.0) / (inference_ms as f32)
        } else {
            0.0
        };

        let raw_output = if res.0.is_empty() {
            "Parakeet TDT returned 0 tokens (empty output)".to_string()
        } else {
            format!("Parakeet TDT Output: \"{}\"", res.0)
        };

        let raw_text = res.0.clone();
        let post = crate::post_processor::post_process(&raw_text, crate::post_processor::PostProcessorConfig {
            normalization: crate::text_normalizer::NormalizationOptions {
                enabled: req.text_normalization,
                remove_fillers: req.remove_filler_words,
                convert_numbers: req.convert_numbers,
                remove_stutters: req.remove_stutters,
            },
            apply_self_corrections: req.apply_self_corrections,
            remove_noise_markers: req.remove_noise_markers,
            collapse_redundancy: req.collapse_redundancy,
            annotate_ambiguity: req.annotate_ambiguity,
            normalize_structured_values: req.normalize_structured_values,
        });
        let final_clean = if !post.clean_text.is_empty() { post.clean_text.clone() } else { raw_text.clone() };
        let post_applied = {
            let mut v = Vec::new();
            if req.text_normalization { v.push("normalization".to_string()); }
            if req.apply_self_corrections { v.push("self_corrections".to_string()); }
            if req.remove_noise_markers { v.push("noise_markers".to_string()); }
            if req.collapse_redundancy { v.push("redundancy".to_string()); }
            if req.annotate_ambiguity { v.push("ambiguity".to_string()); }
            if req.normalize_structured_values { v.push("structured".to_string()); }
            v
        };

        Ok(LabExperimentResult {
            clean_text: final_clean,
            raw_text: raw_text.clone(),
            raw_output,
            detected_lang: res.1,
            segments: vec![DiagnosticSegment {
                start_ms: 0,
                end_ms: (processed_duration_sec * 1000.0) as i64,
                text: raw_text,
                no_speech_prob: 0.0,
            }],
            original_duration_sec,
            processed_duration_sec,
            vad_cut_sec,
            vad_time_ms,
            inference_time_ms: inference_ms.max(res.2),
            speed_factor,
            hardware_engine: "ONNX Runtime (CPU FastConformer)".to_string(),
            original_audio_filename: req.audio_filename.clone(),
            vad_audio_filename,
            post_applied,
            uncertain_spans: post.uncertain_spans.clone(),
            requires_clarification: post.requires_clarification,
        })
    } else {
        // Single Model Policy: unload Parakeet
        {
            let mut pk = parakeet_transcriber.lock().unwrap();
            pk.unload();
        }
        let model_path = Transcriber::ensure_model(app_handle, &req.model_name).await?;
        let mut tr = transcriber.lock().unwrap();

        let lang_opt = req.language.as_deref();
        let res = tr.transcribe_detailed(&processed_samples, &model_path, false, lang_opt)?;

        let speed_factor = if res.inference_ms > 0 {
            (processed_duration_sec * 1000.0) / (res.inference_ms as f32)
        } else {
            0.0
        };

        let hardware_engine = if Transcriber::is_metal_supported() {
            "Apple Silicon Metal GPU (Active)".to_string()
        } else {
            "CPU (NEON SIMD)".to_string()
        };

        let raw_text = res.text.clone();
        let post = crate::post_processor::post_process(&raw_text, crate::post_processor::PostProcessorConfig {
            normalization: crate::text_normalizer::NormalizationOptions {
                enabled: req.text_normalization,
                remove_fillers: req.remove_filler_words,
                convert_numbers: req.convert_numbers,
                remove_stutters: req.remove_stutters,
            },
            apply_self_corrections: req.apply_self_corrections,
            remove_noise_markers: req.remove_noise_markers,
            collapse_redundancy: req.collapse_redundancy,
            annotate_ambiguity: req.annotate_ambiguity,
            normalize_structured_values: req.normalize_structured_values,
        });
        let final_clean = if !post.clean_text.is_empty() { post.clean_text.clone() } else { raw_text.clone() };
        let post_applied = {
            let mut v = Vec::new();
            if req.text_normalization { v.push("normalization".to_string()); }
            if req.apply_self_corrections { v.push("self_corrections".to_string()); }
            if req.remove_noise_markers { v.push("noise_markers".to_string()); }
            if req.collapse_redundancy { v.push("redundancy".to_string()); }
            if req.annotate_ambiguity { v.push("ambiguity".to_string()); }
            if req.normalize_structured_values { v.push("structured".to_string()); }
            v
        };

        Ok(LabExperimentResult {
            clean_text: final_clean,
            raw_text: raw_text.clone(),
            raw_output: res.raw_output.clone(),
            detected_lang: res.detected_lang.clone(),
            segments: res.segments.clone(),
            original_duration_sec,
            processed_duration_sec,
            vad_cut_sec,
            vad_time_ms,
            inference_time_ms: res.inference_ms,
            speed_factor,
            hardware_engine,
            original_audio_filename: req.audio_filename.clone(),
            vad_audio_filename,
            post_applied,
            uncertain_spans: post.uncertain_spans.clone(),
            requires_clarification: post.requires_clarification,
        })
    }
}
