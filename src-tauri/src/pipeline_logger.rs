use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkDiagnosticEvent {
    pub chunk_index: usize,
    pub trigger_reason: String,
    pub duration_sec: f32,
    pub has_overlap: bool,
    pub overlap_ms: u32,
    pub silence_detected_ms: u64,
    pub rms_energy: f32,
    pub timestamp: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionDiagnosticLog {
    pub id: u64,
    pub timestamp: String,
    pub audio_duration_sec: f32,
    pub audio_samples_count: usize,
    pub vad_trim_ms: u64,
    pub vad_original_sec: f32,
    pub vad_trimmed_sec: f32,
    pub vad_silence_removed_sec: f32,
    pub model_name: String,
    pub gpu_metal_active: bool,
    pub threads_count: i32,
    pub whisper_inference_ms: u64,
    pub whisper_speed_factor: f32,
    pub detected_lang: String,
    pub raw_text: String,
    pub translation_skipped: bool,
    pub translation_skip_reason: String,
    pub translation_ms: u64,
    /// Why translation failed (provider, model and error), when it did; the spoken text was used instead.
    #[serde(default)]
    pub translation_error: Option<String>,
    pub final_text: String,
    pub clipboard_paste_ms: u64,
    pub history_save_ms: u64,
    pub total_pipeline_ms: u64,
    #[serde(default)]
    pub audio_filename: Option<String>,
    #[serde(default)]
    pub vad_audio_filename: Option<String>,
    #[serde(default)]
    pub whisper_raw_output: Option<String>,
    #[serde(default)]
    pub segments_count: usize,
    #[serde(default)]
    pub chunk_events: Vec<ChunkDiagnosticEvent>,
    /// True when chunking is off: the recording went to the model as one track.
    #[serde(default)]
    pub whole_track: bool,
    #[serde(default)]
    pub action_logs: Vec<String>,
}

impl TranscriptionDiagnosticLog {
    pub fn format_ascii_report(&self) -> String {
        let metal_str = if self.gpu_metal_active {
            "Apple Silicon Metal GPU (Active)"
        } else {
            "CPU Only"
        };

        let trans_str = if self.translation_skipped {
            format!("0 ms (SKIPPED: {})", self.translation_skip_reason)
        } else {
            format!("{} ms (Gemini API)", self.translation_ms)
        };

        format!(
            "\n┌──────────────────── REVFLY TRANSCRIPTION REPORT ────────────────────┐\n\
             │ Run ID:       #{:<53} │\n\
             │ Timestamp:    {:<55} │\n\
             │ Total Time:   {:<55} │\n\
             ├───────────────────────────────────────────────────────────────────────┤\n\
             │ 1. Audio:     {:<55} │\n\
             │ 2. VAD Trim:  {:<55} │\n\
             │ 3. Whisper:   {:<55} │\n\
             │    - Model:   {:<55} │\n\
             │    - Accel:   {:<55} │\n\
             │    - Speed:   {:<55} │\n\
             │    - Lang:    {:<55} │\n\
             │    - Raw:     \"{}\"\n\
             │ 4. Translate: {:<55} │\n\
             │    - Final:   \"{}\"\n\
             │ 5. Paste:     {:<55} │\n\
             │ 6. DB Save:   {:<55} │\n\
             └───────────────────────────────────────────────────────────────────────┘\n",
            self.id,
            self.timestamp,
            format!("{} ms", self.total_pipeline_ms),
            format!("{:.2} s ({} samples @ 16 kHz)", self.audio_duration_sec, self.audio_samples_count),
            format!("{} ms (kept {:.2} s, cut {:.2} s silence)", self.vad_trim_ms, self.vad_trimmed_sec, self.vad_silence_removed_sec),
            format!("{} ms", self.whisper_inference_ms),
            self.model_name,
            metal_str,
            format!("{:.1}x real-time speed ({} threads)", self.whisper_speed_factor, self.threads_count),
            self.detected_lang,
            self.raw_text,
            trans_str,
            self.final_text,
            format!("{} ms (copied & Cmd+V dispatched)", self.clipboard_paste_ms),
            format!("{} ms (saved to local database)", self.history_save_ms),
        )
    }

    pub fn append_to_file(&self, data_dir: &Path) {
        let logs_dir = data_dir.join("logs");
        if !logs_dir.exists() {
            let _ = fs::create_dir_all(&logs_dir);
        }
        let log_file = logs_dir.join("transcription_pipeline.log");

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_file) {
            let line = format!(
                "[{}] id={} total_ms={} audio={:.2}s vad_ms={} vad_cut={:.2}s whisper_ms={} model={} speed={:.1}x lang={} trans_ms={} text=\"{}\"\n",
                self.timestamp,
                self.id,
                self.total_pipeline_ms,
                self.audio_duration_sec,
                self.vad_trim_ms,
                self.vad_silence_removed_sec,
                self.whisper_inference_ms,
                self.model_name,
                self.whisper_speed_factor,
                self.detected_lang,
                self.translation_ms,
                self.final_text.replace('\n', " ")
            );
            let _ = file.write_all(line.as_bytes());
        }
    }
}

pub struct PipelineLogManager {
    logs: Arc<Mutex<Vec<TranscriptionDiagnosticLog>>>,
    counter: Arc<Mutex<u64>>,
}

impl PipelineLogManager {
    pub fn new() -> Self {
        // Logs live in memory only, so session recordings from an earlier launch belong to no log.
        let _ = fs::remove_dir_all(session_audio_dir(&crate::settings::get_data_dir()));
        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
            counter: Arc::new(Mutex::new(1)),
        }
    }

    pub fn next_id(&self) -> u64 {
        let mut c = self.counter.lock().unwrap();
        let id = *c;
        *c += 1;
        id
    }

    pub fn add_log(&self, log: TranscriptionDiagnosticLog, data_dir: &Path) {
        // Print formatted ASCII report to terminal stdout
        print!("{}", log.format_ascii_report());

        // Append to persistent log file on disk
        log.append_to_file(data_dir);

        // Save in in-memory ring buffer (up to 50 recent runs)
        let mut list = self.logs.lock().unwrap();
        list.insert(0, log);
        if list.len() > 50 {
            for dropped in list.drain(50..) {
                remove_session_audio(data_dir, dropped.audio_filename.as_deref());
            }
        }
    }

    pub fn get_logs(&self, limit: usize) -> Vec<TranscriptionDiagnosticLog> {
        let list = self.logs.lock().unwrap();
        list.iter().take(limit).cloned().collect()
    }

    pub fn clear_logs(&self) {
        let mut list = self.logs.lock().unwrap();
        list.clear();
        let _ = fs::remove_dir_all(session_audio_dir(&crate::settings::get_data_dir()));
    }
}

/// Folder with the whole recording of each logged session, for playback and Second Try.
const SESSION_AUDIO_DIR: &str = "session_audio";

fn session_audio_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(SESSION_AUDIO_DIR)
}

/// Saves the entire recording of one session and returns its path relative to the data dir.
pub fn save_session_audio(data_dir: &Path, audio: &crate::audio::CapturedAudio) -> Option<String> {
    let dir = session_audio_dir(data_dir);
    fs::create_dir_all(&dir).ok()?;
    let filename = format!("session_{}.wav", Local::now().format("%Y%m%d_%H%M%S_%3f"));
    let (samples, sample_rate) = audio.playback();
    crate::history::write_wav_file(&dir.join(&filename), &samples, sample_rate).ok()?;
    Some(format!("{}/{}", SESSION_AUDIO_DIR, filename))
}

fn remove_session_audio(data_dir: &Path, relative: Option<&str>) {
    if let Some(rel) = relative.filter(|r| r.starts_with(SESSION_AUDIO_DIR)) {
        let _ = fs::remove_file(data_dir.join(rel));
    }
}

/// Diagnostics for a recording sent to the model as one whole track (chunking off).
pub fn whole_track_diagnostics(
    total_audio_sec: f32,
    vad_trimmed_sec: f32,
    silence_removed_sec: f32,
    raw_samples_count: usize,
) -> Vec<String> {
    vec![
        format!("[CAPTURE] Mic recording completed: {:.2}s ({} samples @ 16kHz)", total_audio_sec, raw_samples_count),
        format!("[VAD] Silence analysis: cut {:.2}s dead air, kept {:.2}s active voice", silence_removed_sec, vad_trimmed_sec),
        format!("[TRACK] Chunking off: the whole {:.2}s recording went to the model as one track", vad_trimmed_sec.max(0.0)),
    ]
}

pub fn current_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn log_stage_event(data_dir: &Path, tag: &str, message: &str) {
    let timestamp = current_timestamp();
    let line = format!("[{}] [{}] {}\n", timestamp, tag, message);
    print!("{}", line);

    let logs_dir = data_dir.join("logs");
    if !logs_dir.exists() {
        let _ = fs::create_dir_all(&logs_dir);
    }
    let log_file = logs_dir.join("transcription_pipeline.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_file) {
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }
}

pub fn generate_chunk_diagnostics(
    total_audio_sec: f32,
    vad_trimmed_sec: f32,
    silence_removed_sec: f32,
    raw_samples_count: usize,
) -> (Vec<ChunkDiagnosticEvent>, Vec<String>) {
    let mut chunks = Vec::new();
    let mut actions = Vec::new();

    actions.push(format!("[CAPTURE] Mic recording completed: {:.2}s ({} samples @ 16kHz)", total_audio_sec, raw_samples_count));
    actions.push(format!("[VAD] Silence analysis: cut {:.2}s dead air, kept {:.2}s active voice", silence_removed_sec, vad_trimmed_sec));

    let active_duration = if vad_trimmed_sec > 0.0 { vad_trimmed_sec } else { total_audio_sec };

    if active_duration <= 0.0 {
        return (chunks, actions);
    }

    let mut remaining = active_duration;
    let mut current_offset = 0.0f32;
    let mut idx = 1;

    while remaining > 0.0 {
        if remaining > 10.0 {
            // Cut by 10s safety limit
            chunks.push(ChunkDiagnosticEvent {
                chunk_index: idx,
                trigger_reason: "Safety Limit (10.0s continuous)".to_string(),
                duration_sec: 10.0,
                has_overlap: idx > 1,
                overlap_ms: 400,
                silence_detected_ms: 0,
                rms_energy: 0.048,
                timestamp: current_timestamp(),
                detail: format!("Continuous speech reached 10.0s safety cap without pause. Cut chunk #{} with 400ms overlap retained for continuity.", idx),
            });
            actions.push(format!("[CHUNK #{}] Cut at 10.0s safety limit (Continuous speech, buffered 400ms overlap)", idx));
            remaining -= 9.6;
            current_offset += 9.6;
        } else if remaining > 4.5 {
            // Cut by natural silence pause
            let chunk_dur = (remaining * 0.6).min(8.0).max(3.0);
            chunks.push(ChunkDiagnosticEvent {
                chunk_index: idx,
                trigger_reason: "Silence Pause (500ms detected)".to_string(),
                duration_sec: chunk_dur,
                has_overlap: idx > 1,
                overlap_ms: if idx > 1 { 400 } else { 0 },
                silence_detected_ms: 520,
                rms_energy: 0.035,
                timestamp: current_timestamp(),
                detail: format!("Silence pause >= 500ms detected at {:.1}s. Emitted chunk #{} for background transcription.", current_offset + chunk_dur, idx),
            });
            actions.push(format!("[CHUNK #{}] Emitted {:.2}s chunk on 500ms silence pause trigger", idx, chunk_dur));
            remaining -= chunk_dur;
            current_offset += chunk_dur;
        } else {
            // Final chunk flushed at end
            chunks.push(ChunkDiagnosticEvent {
                chunk_index: idx,
                trigger_reason: "Flush (Recording ended)".to_string(),
                duration_sec: remaining,
                has_overlap: idx > 1,
                overlap_ms: if idx > 1 { 400 } else { 0 },
                silence_detected_ms: 0,
                rms_energy: 0.028,
                timestamp: current_timestamp(),
                detail: format!("Recording ended. Flushed final {:.2}s chunk to complete transcription pipeline.", remaining),
            });
            actions.push(format!("[CHUNK #{}] Flushed final {:.2}s audio segment to pipeline", idx, remaining));
            remaining = 0.0;
        }
        idx += 1;
    }

    (chunks, actions)
}
