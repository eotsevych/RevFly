use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub enum AppSound {
    StartRecording,
    TranscriptionComplete,
}

/// Plays audio earcon asynchronously.
/// Supports macOS (afplay), Linux (paplay/aplay), and Windows (PowerShell Media.SoundPlayer).
pub fn play_sound(sound: AppSound) {
    std::thread::spawn(move || {
        let (filename, bytes) = match sound {
            AppSound::StartRecording => (
                "start_recording.wav",
                include_bytes!("../sounds/start_recording.wav").as_slice(),
            ),
            AppSound::TranscriptionComplete => (
                "transcription_complete.wav",
                include_bytes!("../sounds/transcription_complete.wav").as_slice(),
            ),
        };

        let sound_dir: PathBuf = crate::settings::get_data_dir().join("sounds");
        let _ = std::fs::create_dir_all(&sound_dir);
        let sound_path = sound_dir.join(filename);

        // Always ensure sound file exists on disk and is up to date
        if !sound_path.exists() || std::fs::read(&sound_path).map(|d| d != bytes).unwrap_or(true) {
            let _ = std::fs::write(&sound_path, bytes);
        }

        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("afplay")
                .arg(&sound_path)
                .spawn();
        }

        #[cfg(target_os = "linux")]
        {
            if std::process::Command::new("paplay")
                .arg(&sound_path)
                .spawn()
                .is_err()
            {
                let _ = std::process::Command::new("aplay")
                    .arg(&sound_path)
                    .spawn();
            }
        }

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            let ps_script = format!(
                "(New-Object Media.SoundPlayer '{}').Play()",
                sound_path.to_string_lossy().replace('\'', "''")
            );
            let _ = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
    });
}
