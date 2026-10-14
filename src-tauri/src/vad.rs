use silero_vad_crs::{
    get_timestamps_from_probs_with_config, SileroVad, TimestampConfig,
};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct VadResult {
    pub samples: Vec<f32>,
    pub original_samples: usize,
    pub trimmed_samples: usize,
    pub original_duration_sec: f32,
    pub trimmed_duration_sec: f32,
    pub silence_removed_sec: f32,
    pub duration_ms: u64,
    /// Silero found no speech, so the whole recording was kept. False when VAD couldn't run.
    pub no_speech: bool,
}

/// Trims silence from 16 kHz mono audio using Silero VAD.
/// Keeps 400ms padding before voice start and after voice end.
/// If no speech is detected, falls back to the full original buffer.
pub fn trim_silence(samples: &[f32], sample_rate: u32) -> VadResult {
    let t0 = Instant::now();
    let original_len = samples.len();
    let original_dur = original_len as f32 / sample_rate as f32;

    if samples.is_empty() {
        return VadResult {
            samples: Vec::new(),
            original_samples: 0,
            trimmed_samples: 0,
            original_duration_sec: 0.0,
            trimmed_duration_sec: 0.0,
            silence_removed_sec: 0.0,
            duration_ms: t0.elapsed().as_millis() as u64,
            no_speech: false,
        };
    }

    let mut vad = match SileroVad::new() {
        Ok(v) => v,
        Err(e) => {
            log::warn!("Silero VAD init error: {:?}. Returning full buffer.", e);
            let ms = t0.elapsed().as_millis() as u64;
            return VadResult {
                samples: samples.to_vec(),
                original_samples: original_len,
                trimmed_samples: original_len,
                original_duration_sec: original_dur,
                trimmed_duration_sec: original_dur,
                silence_removed_sec: 0.0,
                duration_ms: ms,
                no_speech: false,
            };
        }
    };

    let probs = match vad.forward_audio(samples) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("Silero VAD forward error: {:?}. Returning full buffer.", e);
            let ms = t0.elapsed().as_millis() as u64;
            return VadResult {
                samples: samples.to_vec(),
                original_samples: original_len,
                trimmed_samples: original_len,
                original_duration_sec: original_dur,
                trimmed_duration_sec: original_dur,
                silence_removed_sec: 0.0,
                duration_ms: ms,
                no_speech: false,
            };
        }
    };

    let segments = get_timestamps_from_probs_with_config(&probs, samples.len(), timestamp_config(sample_rate));

    let no_speech = segments.is_empty();
    let trimmed = if no_speech {
        log::info!("Silero VAD detected no active speech, preserving full buffer as fallback.");
        samples.to_vec()
    } else {
        // Head-only trimming:
        // Trim silence before the first word (saving model inference time).
        // NEVER trim the ending of user-triggered audio recordings to avoid cutting off trailing syllables or words.
        let first = segments.first().unwrap();
        let start = first.start.min(samples.len());

        if start < samples.len() {
            samples[start..].to_vec()
        } else {
            samples.to_vec()
        }
    };

    let trimmed_len = trimmed.len();
    let trimmed_dur = trimmed_len as f32 / sample_rate as f32;
    let silence_removed = (original_dur - trimmed_dur).max(0.0);
    let ms = t0.elapsed().as_millis() as u64;

    VadResult {
        samples: trimmed,
        original_samples: original_len,
        trimmed_samples: trimmed_len,
        original_duration_sec: original_dur,
        trimmed_duration_sec: trimmed_dur,
        silence_removed_sec: silence_removed,
        duration_ms: ms,
        no_speech,
    }
}

fn timestamp_config(sample_rate: u32) -> TimestampConfig {
    let mut config = TimestampConfig::default();
    config.sampling_rate = sample_rate as usize;
    config.speech_pad_ms = 400; // Keep 400ms margin before voice start and after voice end
    config.threshold = 0.25;    // Sensitive threshold to capture soft speech and whispers
    config.min_speech_duration_ms = 80;
    config.min_silence_duration_ms = 400;
    config
}

/// Whether 16 kHz mono audio contains any speech, judged like `trim_silence`. Says yes when the
/// VAD can't run, so a failure never raises a false "no speech" alarm.
pub fn has_speech(samples: &[f32]) -> bool {
    if samples.is_empty() {
        return false;
    }
    let Ok(mut vad) = SileroVad::new() else {
        return true;
    };
    let Ok(probs) = vad.forward_audio(samples) else {
        return true;
    };
    !get_timestamps_from_probs_with_config(&probs, samples.len(), timestamp_config(16000)).is_empty()
}
