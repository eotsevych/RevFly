use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::settings::get_data_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub id: i64,
    pub timestamp: String,
    pub raw_text: String,
    pub translated_text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub duration: f32,
    pub audio_path: Option<String>,
}

pub struct HistoryManager {
    db_conn: Mutex<Connection>,
    audio_dir: PathBuf,
}

impl HistoryManager {
    pub fn new() -> Result<Self, String> {
        let data_dir = get_data_dir();
        if !data_dir.exists() {
            let _ = fs::create_dir_all(&data_dir);
        }

        let audio_dir = data_dir.join("audio");
        if !audio_dir.exists() {
            let _ = fs::create_dir_all(&audio_dir);
        }

        let db_path = data_dir.join("history.db");
        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                raw_text TEXT NOT NULL,
                translated_text TEXT NOT NULL,
                source_lang TEXT NOT NULL,
                target_lang TEXT NOT NULL,
                duration REAL NOT NULL,
                audio_path TEXT
            )",
            [],
        )
        .map_err(|e| e.to_string())?;

        Ok(Self {
            db_conn: Mutex::new(conn),
            audio_dir,
        })
    }

    pub fn add_entry(
        &self,
        raw_text: &str,
        translated_text: &str,
        source_lang: &str,
        target_lang: &str,
        duration: f32,
        audio_samples: Option<&[f32]>,
        storage_mode: &str,
        storage_cap_mb: u64,
    ) -> Result<i64, String> {
        if storage_mode == "private" || storage_mode == "private_mode" {
            // Private mode: do not save anything to disk
            return Ok(0);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let timestamp_str = format!("{}", now);

        let mut audio_file_rel: Option<String> = None;

        if storage_mode == "text_audio" || storage_mode == "text_and_audio" {
            if let Some(samples) = audio_samples {
                let filename = format!("rec_{}.wav", now);
                let full_path = self.audio_dir.join(&filename);

                if let Ok(()) = write_wav_file(&full_path, samples, 16000) {
                    audio_file_rel = Some(filename);
                }

                // Check and enforce storage cap after adding audio
                let _ = self.enforce_storage_cap(storage_cap_mb);
            }
        }

        let conn = self.db_conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO history (timestamp, raw_text, translated_text, source_lang, target_lang, duration, audio_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                timestamp_str,
                raw_text,
                translated_text,
                source_lang,
                target_lang,
                duration,
                audio_file_rel,
            ],
        )
        .map_err(|e| e.to_string())?;

        let row_id = conn.last_insert_rowid();
        Ok(row_id)
    }

    pub fn get_records(&self, limit: usize) -> Result<Vec<HistoryRecord>, String> {
        let conn = self.db_conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, timestamp, raw_text, translated_text, source_lang, target_lang, duration, audio_path
                 FROM history ORDER BY id DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(HistoryRecord {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    raw_text: row.get(2)?,
                    translated_text: row.get(3)?,
                    source_lang: row.get(4)?,
                    target_lang: row.get(5)?,
                    duration: row.get(6)?,
                    audio_path: row.get(7)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut list = Vec::new();
        for r in rows {
            if let Ok(rec) = r {
                list.push(rec);
            }
        }
        Ok(list)
    }

    pub fn clear_history(&self) -> Result<(), String> {
        let conn = self.db_conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM history", [])
            .map_err(|e| e.to_string())?;

        // Delete all audio files in audio directory
        if let Ok(entries) = fs::read_dir(&self.audio_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let _ = fs::remove_file(p);
                }
            }
        }

        // Also delete latest_recording.wav and latest_vad_trimmed.wav from data_dir
        let data_dir = get_data_dir();
        let latest = data_dir.join("latest_recording.wav");
        if latest.exists() {
            let _ = fs::remove_file(latest);
        }
        let vad = data_dir.join("latest_vad_trimmed.wav");
        if vad.exists() {
            let _ = fs::remove_file(vad);
        }

        // Also clean any stray wav files in data_dir
        if let Ok(entries) = fs::read_dir(&data_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension() {
                        if ext == "wav" {
                            let _ = fs::remove_file(p);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn enforce_retention_policy(&self, retention_days: u32) -> Result<(), String> {
        if retention_days == 0 {
            return Ok(()); // 0 means Never
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cutoff = now.saturating_sub((retention_days as u64) * 86400);

        let conn = self.db_conn.lock().map_err(|e| e.to_string())?;

        // Find audio paths to delete
        let mut stmt = conn
            .prepare("SELECT audio_path FROM history WHERE CAST(timestamp AS INTEGER) < ?1 AND audio_path IS NOT NULL")
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![cutoff as i64], |row| {
                row.get::<_, Option<String>>(0)
            })
            .map_err(|e| e.to_string())?;

        for r in rows.flatten().flatten() {
            let full_p = self.audio_dir.join(r);
            if full_p.exists() {
                let _ = fs::remove_file(full_p);
            }
        }

        conn.execute(
            "DELETE FROM history WHERE CAST(timestamp AS INTEGER) < ?1",
            params![cutoff as i64],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn enforce_storage_cap(&self, cap_mb: u64) -> Result<(), String> {
        let max_bytes = cap_mb * 1024 * 1024;
        let mut total_size: u64 = 0;
        let mut files_with_meta: Vec<(PathBuf, SystemTime, u64)> = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.audio_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Ok(meta) = p.metadata() {
                        let size = meta.len();
                        total_size += size;
                        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
                        files_with_meta.push((p, modified, size));
                    }
                }
            }
        }

        if total_size <= max_bytes {
            return Ok(());
        }

        // Sort files by modified time ascending (oldest first - FIFO)
        files_with_meta.sort_by_key(|item| item.1);

        for (path, _, size) in files_with_meta {
            if total_size <= max_bytes {
                break;
            }
            if let Ok(()) = fs::remove_file(&path) {
                total_size = total_size.saturating_sub(size);
                // Nullify audio_path in DB if needed
                if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                    if let Ok(conn) = self.db_conn.lock() {
                        let _ = conn.execute(
                            "UPDATE history SET audio_path = NULL WHERE audio_path = ?1",
                            params![filename],
                        );
                    }
                }
            }
        }

        Ok(())
    }

    pub fn get_audio_dir(&self) -> PathBuf {
        self.audio_dir.clone()
    }

    pub fn get_audio_file_path(&self, filename: &str) -> PathBuf {
        self.audio_dir.join(filename)
    }
}

pub fn write_wav_file(path: &Path, samples: &[f32], sample_rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for &sample in samples {
        let val = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer.write_sample(val).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    Ok(())
}
