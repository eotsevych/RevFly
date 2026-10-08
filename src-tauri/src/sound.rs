use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub enum AppSound {
    StartRecording,
    StopRecording,
}

/// Mono samples of an earcon, decoded once from the embedded WAV.
struct Earcon {
    samples: Vec<f32>,
    sample_rate: u32,
}

fn decode_wav(bytes: &[u8]) -> Earcon {
    let Ok(reader) = hound::WavReader::new(std::io::Cursor::new(bytes)) else {
        return Earcon { samples: Vec::new(), sample_rate: 44100 };
    };
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.into_samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample.max(1) - 1)) as f32;
            reader.into_samples::<i32>().filter_map(Result::ok).map(|v| v as f32 / scale).collect()
        }
    };
    let samples = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    Earcon { samples, sample_rate: spec.sample_rate }
}

fn earcon(sound: AppSound) -> &'static Earcon {
    static START: OnceLock<Earcon> = OnceLock::new();
    static STOP: OnceLock<Earcon> = OnceLock::new();
    match sound {
        AppSound::StartRecording => START.get_or_init(|| decode_wav(include_bytes!("../sounds/start_recording.wav"))),
        AppSound::StopRecording => STOP.get_or_init(|| decode_wav(include_bytes!("../sounds/transcription_complete.wav"))),
    }
}

/// Decodes the earcons ahead of time so the first key press plays without delay.
pub fn preload() {
    earcon(AppSound::StartRecording);
    earcon(AppSound::StopRecording);
}

/// Plays an earcon asynchronously, in-process, through the default output device.
/// Opening an output stream takes a few milliseconds, so the sound starts in step with the key press
/// (spawning an external player such as afplay added a noticeable delay).
pub fn play_sound(sound: AppSound) {
    std::thread::spawn(move || {
        if let Err(e) = play_earcon(earcon(sound)) {
            log::warn!("Failed to play {:?} sound: {}", sound, e);
        }
    });
}

fn play_earcon(clip: &'static Earcon) -> Result<(), String> {
    if clip.samples.is_empty() {
        return Ok(());
    }
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| "No audio output device".to_string())?;
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let out_rate = config.sample_rate().0;
    let channels = config.channels().max(1) as usize;

    // Linear resampling from the clip rate to the device rate.
    let step = clip.sample_rate as f64 / out_rate as f64;
    let total_frames = (clip.samples.len() as f64 / step) as usize;
    let frame_pos = Arc::new(AtomicUsize::new(0));
    let pos_cb = Arc::clone(&frame_pos);
    let sample_at = move |frame: usize| -> f32 {
        let src = frame as f64 * step;
        let i = src as usize;
        let frac = (src - i as f64) as f32;
        let a = clip.samples.get(i).copied().unwrap_or(0.0);
        let b = clip.samples.get(i + 1).copied().unwrap_or(a);
        a + (b - a) * frac
    };

    let err_fn = |e| log::warn!("Sound output stream error: {}", e);
    let stream_config: cpal::StreamConfig = config.clone().into();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _: &_| {
                let start = pos_cb.fetch_add(data.len() / channels, Ordering::Relaxed);
                for (n, frame) in data.chunks_mut(channels).enumerate() {
                    let v = if start + n < total_frames { sample_at(start + n) } else { 0.0 };
                    frame.fill(v);
                }
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_output_stream(
            &stream_config,
            move |data: &mut [i16], _: &_| {
                let start = pos_cb.fetch_add(data.len() / channels, Ordering::Relaxed);
                for (n, frame) in data.chunks_mut(channels).enumerate() {
                    let v = if start + n < total_frames { sample_at(start + n) } else { 0.0 };
                    frame.fill((v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
                }
            },
            err_fn,
            None,
        ),
        fmt => return Err(format!("Unsupported output sample format: {:?}", fmt)),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;

    // Keep the stream alive until the clip has been played out, plus a short tail for device latency.
    let clip_ms = (total_frames as u64 * 1000) / out_rate.max(1) as u64;
    std::thread::sleep(Duration::from_millis(clip_ms + 150));
    Ok(())
}

/// Plays a WAV file with the platform's built-in player without blocking the caller.
pub fn play_wav_file(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("afplay")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to play audio with afplay: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        if std::process::Command::new("paplay").arg(path).spawn().is_err() {
            std::process::Command::new("aplay")
                .arg(path)
                .spawn()
                .map_err(|e| format!("Failed to play audio with paplay/aplay: {}", e))?;
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        // PlaySync keeps PowerShell alive until playback ends; Play() would exit and cut the sound off.
        let ps_script = format!(
            "(New-Object Media.SoundPlayer '{}').PlaySync()",
            path.to_string_lossy().replace('\'', "''")
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to play audio with PowerShell: {}", e))?;
    }

    Ok(())
}
