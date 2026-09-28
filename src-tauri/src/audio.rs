use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut names = Vec::new();

    // Query all host devices and filter those supporting input
    if let Ok(devices) = host.devices() {
        for d in devices {
            let has_input = d.default_input_config().is_ok()
                || d.supported_input_configs().map(|mut c| c.next().is_some()).unwrap_or(false);
            if has_input {
                if let Ok(name) = d.name() {
                    let trimmed = name.trim().to_string();
                    if !trimmed.is_empty() && !names.contains(&trimmed) {
                        names.push(trimmed);
                    }
                }
            }
        }
    }

    // Also include any devices from host.input_devices()
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() && !names.contains(&trimmed) {
                    names.push(trimmed);
                }
            }
        }
    }

    names
}

pub fn list_output_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut names = Vec::new();

    // Query all host devices and filter those supporting output
    if let Ok(devices) = host.devices() {
        for d in devices {
            let has_output = d.default_output_config().is_ok()
                || d.supported_output_configs().map(|mut c| c.next().is_some()).unwrap_or(false);
            if has_output {
                if let Ok(name) = d.name() {
                    let trimmed = name.trim().to_string();
                    if !trimmed.is_empty() && !names.contains(&trimmed) {
                        names.push(trimmed);
                    }
                }
            }
        }
    }

    if let Ok(devices) = host.output_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() && !names.contains(&trimmed) {
                    names.push(trimmed);
                }
            }
        }
    }

    names
}

pub fn resolve_input_device(device_name: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if let Some(target) = device_name {
        let clean = target.trim();
        if clean != "Default" && clean != "System default" && clean != "System Default" && !clean.is_empty() {
            if let Ok(devs) = host.devices() {
                for d in devs {
                    if let Ok(name) = d.name() {
                        if name.trim() == clean {
                            return Some(d);
                        }
                    }
                }
            }
            if let Ok(mut devs) = host.input_devices() {
                if let Some(found) = devs.find(|d| d.name().map(|n| n.trim() == clean).unwrap_or(false)) {
                    return Some(found);
                }
            }
        }
    }
    host.default_input_device()
}

static MIC_TEST_STOP: Mutex<Option<Sender<()>>> = Mutex::new(None);

pub fn stop_mic_test() {
    if let Ok(mut lock) = MIC_TEST_STOP.lock() {
        if let Some(tx) = lock.take() {
            let _ = tx.send(());
        }
    }
}

pub fn start_mic_test(app_handle: AppHandle, device_name: Option<String>) -> Result<(), String> {
    stop_mic_test();

    let (stop_tx, stop_rx) = channel::<()>();
    let (init_tx, init_rx) = channel::<Result<(), String>>();

    if let Ok(mut lock) = MIC_TEST_STOP.lock() {
        *lock = Some(stop_tx);
    }

    thread::spawn(move || {
        let device = match resolve_input_device(device_name.as_deref()) {
            Some(d) => d,
            None => {
                let _ = init_tx.send(Err("No audio input device found".to_string()));
                return;
            }
        };

        let supported_config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => match device.supported_input_configs().ok().and_then(|mut iter| iter.next()) {
                Some(conf) => conf.with_max_sample_rate(),
                None => {
                    let _ = init_tx.send(Err(format!("Failed to get default input config: {}", e)));
                    return;
                }
            },
        };

        let sample_format = supported_config.sample_format();
        let channels = supported_config.channels() as usize;
        let latest_rms = Arc::new(Mutex::new(0.0f32));
        let rms_write = Arc::clone(&latest_rms);
        let running = Arc::new(AtomicBool::new(true));
        let running_cb = Arc::clone(&running);

        let err_fn = |err| {
            log::warn!("Mic test stream error: {}", err);
        };

        let stream_result = match sample_format {
            cpal::SampleFormat::F32 => {
                device.build_input_stream(
                    &supported_config.into(),
                    move |data: &[f32], _: &_| {
                        if !running_cb.load(Ordering::Relaxed) || data.is_empty() {
                            return;
                        }
                        let mut sum_sq = 0.0f32;
                        let step = channels.max(1);
                        let mut count = 0;
                        for i in (0..data.len()).step_by(step) {
                            let s = data[i];
                            sum_sq += s * s;
                            count += 1;
                        }
                        let rms = if count > 0 { (sum_sq / count as f32).sqrt() } else { 0.0 };
                        if let Ok(mut r) = rms_write.lock() {
                            *r = rms;
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                device.build_input_stream(
                    &supported_config.into(),
                    move |data: &[i16], _: &_| {
                        if !running_cb.load(Ordering::Relaxed) || data.is_empty() {
                            return;
                        }
                        let mut sum_sq = 0.0f32;
                        let step = channels.max(1);
                        let mut count = 0;
                        for i in (0..data.len()).step_by(step) {
                            let s = data[i].to_float_sample();
                            sum_sq += s * s;
                            count += 1;
                        }
                        let rms = if count > 0 { (sum_sq / count as f32).sqrt() } else { 0.0 };
                        if let Ok(mut r) = rms_write.lock() {
                            *r = rms;
                        }
                    },
                    err_fn,
                    None,
                )
            }
            fmt => {
                let _ = init_tx.send(Err(format!("Unsupported format: {:?}", fmt)));
                return;
            }
        };

        let stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                let _ = init_tx.send(Err(format!("Failed to build input stream: {}", e)));
                return;
            }
        };

        if let Err(e) = stream.play() {
            let _ = init_tx.send(Err(format!("Failed to play input stream: {}", e)));
            return;
        }

        let _ = init_tx.send(Ok(()));

        let mut peak_hold = 0.0f32;
        loop {
            if stop_rx.recv_timeout(Duration::from_millis(40)).is_ok() {
                break;
            }

            let rms = latest_rms.lock().map(|r| *r).unwrap_or(0.0);
            let level = (rms * 14.0).clamp(0.0, 1.0);
            peak_hold = (peak_hold * 0.93).max(level);

            let _ = app_handle.emit("mic-test-level", serde_json::json!({
                "level": level,
                "peak": peak_hold
            }));
        }

        running.store(false, Ordering::SeqCst);
        let _ = stream.pause();
    });

    init_rx
        .recv()
        .map_err(|e| format!("Mic test thread did not respond: {}", e))?
}

enum AudioCmd {
    Start {
        app_handle: AppHandle,
        device_name: Option<String>,
        reply: Sender<Result<(), String>>,
    },
    Stop {
        reply: Sender<Vec<f32>>,
    },
    Cancel,
}

pub struct AudioRecorder {
    cmd_tx: Sender<AudioCmd>,
    is_recording: Arc<AtomicBool>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = channel::<AudioCmd>();
        let is_recording = Arc::new(AtomicBool::new(false));
        let is_rec_thread = Arc::clone(&is_recording);

        // Dedicated audio recording thread so cpal::Stream stays on one thread
        thread::spawn(move || {
            let mut active_stream: Option<cpal::Stream> = None;
            let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));

            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    AudioCmd::Start { app_handle, device_name, reply } => {
                        stop_mic_test();
                        if let Ok(mut b) = buffer.lock() {
                            b.clear();
                        }

                        // Mark recording active before starting stream so initial audio buffers are captured immediately
                        is_rec_thread.store(true, Ordering::SeqCst);

                        let res = start_stream_inner(
                            &mut active_stream,
                            Arc::clone(&buffer),
                            Arc::clone(&is_rec_thread),
                            app_handle,
                            device_name,
                        );

                        if res.is_err() {
                            is_rec_thread.store(false, Ordering::SeqCst);
                        }
                        let _ = reply.send(res);
                    }
                    AudioCmd::Stop { reply } => {
                        // Allow a brief flush window for final hardware audio frames to arrive from audio callback
                        thread::sleep(Duration::from_millis(120));
                        is_rec_thread.store(false, Ordering::SeqCst);
                        if let Some(stream) = active_stream.take() {
                            let _ = stream.pause();
                        }
                        let collected = buffer.lock().map(|b| b.clone()).unwrap_or_default();
                        let _ = reply.send(collected);
                    }
                    AudioCmd::Cancel => {
                        is_rec_thread.store(false, Ordering::SeqCst);
                        if let Some(stream) = active_stream.take() {
                            let _ = stream.pause();
                        }
                        if let Ok(mut b) = buffer.lock() {
                            b.clear();
                        }
                    }
                }
            }
        });

        Self {
            cmd_tx,
            is_recording,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }

    pub fn start_recording(&self, app_handle: AppHandle, device_name: Option<String>) -> Result<(), String> {
        let (reply_tx, reply_rx) = channel();
        self.cmd_tx
            .send(AudioCmd::Start {
                app_handle,
                device_name,
                reply: reply_tx,
            })
            .map_err(|e| format!("Failed to send start command to audio thread: {}", e))?;

        reply_rx
            .recv()
            .map_err(|e| format!("Audio thread did not respond: {}", e))?
    }

    pub fn stop_recording(&self) -> Vec<f32> {
        let (reply_tx, reply_rx) = channel();
        if self.cmd_tx.send(AudioCmd::Stop { reply: reply_tx }).is_ok() {
            reply_rx.recv().unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    pub fn cancel_recording(&self) {
        let _ = self.cmd_tx.send(AudioCmd::Cancel);
    }
}

fn start_stream_inner(
    active_stream: &mut Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    is_rec: Arc<AtomicBool>,
    app_handle: AppHandle,
    device_name: Option<String>,
) -> Result<(), String> {
    let device = resolve_input_device(device_name.as_deref())
        .ok_or_else(|| "No default audio input device found".to_string())?;

    let supported_config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => match device.supported_input_configs().ok().and_then(|mut iter| iter.next()) {
            Some(conf) => conf.with_max_sample_rate(),
            None => return Err(format!("Failed to get default input config: {}", e)),
        },
    };

    let sample_rate = supported_config.sample_rate().0;
    let channels = supported_config.channels() as usize;
    let sample_format = supported_config.sample_format();

    let resampler_state = Arc::new(Mutex::new(ResamplerState::new()));
    let resampler_clone = Arc::clone(&resampler_state);
    let buffer_clone = Arc::clone(&buffer);
    let is_rec_cb = Arc::clone(&is_rec);

    let err_fn = |err| {
        log::error!("Audio stream error: {}", err);
    };

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let resampler_cb = Arc::clone(&resampler_clone);
            device.build_input_stream(
                &supported_config.into(),
                move |data: &[f32], _: &_| {
                    if !is_rec_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    process_input_samples(data, channels, sample_rate, &resampler_cb, &buffer_clone);
                },
                err_fn,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let resampler_cb = Arc::clone(&resampler_clone);
            device.build_input_stream(
                &supported_config.into(),
                move |data: &[i16], _: &_| {
                    if !is_rec_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    let floats: Vec<f32> = data.iter().map(|&s| s.to_float_sample()).collect();
                    process_input_samples(&floats, channels, sample_rate, &resampler_cb, &buffer_clone);
                },
                err_fn,
                None,
            )
        }
        fmt => {
            return Err(format!("Unsupported audio sample format: {:?}", fmt));
        }
    }
    .map_err(|e| format!("Failed to build input audio stream: {}", e))?;

    stream
        .play()
        .map_err(|e| format!("Failed to play input audio stream: {}", e))?;

    *active_stream = Some(stream);

    // Monitoring thread for animated equalizer levels
    let is_rec_monitor = Arc::clone(&is_rec);
    let buf_monitor = Arc::clone(&buffer);
    thread::spawn(move || {
        let mut last_len = 0;
        let mut phase: f32 = 0.0;

        while is_rec_monitor.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(40));

            let rms = {
                if let Ok(b) = buf_monitor.lock() {
                    let cur_len = b.len();
                    if cur_len > last_len {
                        let window_size = (cur_len - last_len).min(1024);
                        let slice = &b[cur_len.saturating_sub(window_size)..cur_len];
                        last_len = cur_len;
                        let sum_sq: f32 = slice.iter().map(|&x| x * x).sum();
                        (sum_sq / slice.len().max(1) as f32).sqrt()
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            };

            phase += 0.35;
            // Map rms to 0.0..1.0 with speech sensitivity curve
            let norm = (rms * 14.0).clamp(0.0, 1.0);

            let levels: Vec<f32> = [0.0, 1.0, 2.0, 3.0, 4.0]
                .iter()
                .map(|&i| {
                    if norm > 0.02 {
                        let ripple = (phase + i * 1.4).sin() * 0.25;
                        (norm * 0.85 + ripple * norm).clamp(0.05, 1.0)
                    } else {
                        0.0
                    }
                })
                .collect();

            let _ = app_handle.emit("audio-level", levels);
        }
    });

    Ok(())
}

#[derive(Debug)]
pub struct ResamplerState {
    pub acc: Vec<f32>,
    pub phase: f64,
}

impl ResamplerState {
    pub fn new() -> Self {
        Self {
            acc: Vec::with_capacity(4096),
            phase: 0.0,
        }
    }
}

fn process_input_samples(
    data: &[f32],
    channels: usize,
    src_rate: u32,
    resampler: &Arc<Mutex<ResamplerState>>,
    target_buffer: &Arc<Mutex<Vec<f32>>>,
) {
    if channels == 0 || data.is_empty() {
        return;
    }

    let mono_count = data.len() / channels;
    let mut mono_chunk = Vec::with_capacity(mono_count);
    for frame in data.chunks_exact(channels) {
        let sum: f32 = frame.iter().sum();
        mono_chunk.push(sum / channels as f32);
    }

    if src_rate == 16000 {
        if let Ok(mut target) = target_buffer.lock() {
            target.extend_from_slice(&mono_chunk);
        }
    } else {
        if let Ok(mut state) = resampler.lock() {
            state.acc.extend_from_slice(&mono_chunk);

            let ratio = src_rate as f64 / 16000.0;
            // Anti-aliasing low-pass cutoff at 7.2 kHz (target Nyquist is 8.0 kHz)
            let fc = (7200.0 / src_rate as f64).min(0.48);
            const RADIUS: i32 = 16; // 32-tap windowed sinc filter
            let radius_f = RADIUS as f64;

            let mut resampled = Vec::new();
            let mut current_pos = state.phase;

            let acc_len = state.acc.len();
            while current_pos + radius_f < acc_len as f64 {
                let center_idx = current_pos.round() as i32;
                let mut sum = 0.0f32;
                let mut weight_sum = 0.0f32;

                for k in (center_idx - RADIUS)..=(center_idx + RADIUS) {
                    if k >= 0 && (k as usize) < acc_len {
                        let diff = k as f64 - current_pos;
                        // Bandlimited sinc filter with anti-aliasing cutoff
                        let sinc_val = sinc(2.0 * fc * diff);
                        // Hann window
                        let win = 0.5 * (1.0 + (std::f64::consts::PI * diff / radius_f).cos());
                        let w = (sinc_val * win) as f32;

                        sum += state.acc[k as usize] * w;
                        weight_sum += w;
                    }
                }

                if weight_sum.abs() > 1e-6 {
                    resampled.push(sum / weight_sum);
                } else {
                    let idx_clamped = (current_pos.round() as usize).min(acc_len.saturating_sub(1));
                    resampled.push(state.acc[idx_clamped]);
                }

                current_pos += ratio;
            }

            // Keep samples required for the next window tail
            let consumed = current_pos.floor() as usize;
            if consumed > RADIUS as usize {
                let remove_count = consumed - RADIUS as usize;
                let cur_len = state.acc.len();
                state.acc.drain(0..remove_count.min(cur_len));
                state.phase = current_pos - remove_count as f64;
            } else {
                state.phase = current_pos;
            }

            if !resampled.is_empty() {
                if let Ok(mut target) = target_buffer.lock() {
                    target.extend_from_slice(&resampled);
                }
            }
        }
    }
}

#[inline]
fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sinc_resampler_48k_to_16k() {
        let resampler = Arc::new(Mutex::new(ResamplerState::new()));
        let target = Arc::new(Mutex::new(Vec::new()));

        // Generate 4800 samples at 48000 Hz = 0.1 second of 1000 Hz tone
        let mut input = Vec::with_capacity(4800);
        for i in 0..4800 {
            let t = i as f32 / 48000.0;
            input.push((2.0 * std::f32::consts::PI * 1000.0 * t).sin());
        }

        // Feed in 3 chunks to test streaming continuity
        for chunk in input.chunks(1600) {
            process_input_samples(chunk, 1, 48000, &resampler, &target);
        }

        let output = target.lock().unwrap().clone();
        // 0.1 sec at 16000 Hz is ~1600 samples (give or take filter margin)
        assert!(output.len() >= 1550 && output.len() <= 1620, "Output length: {}", output.len());
        for &sample in &output {
            assert!(!sample.is_nan());
            assert!(sample.abs() <= 1.2);
        }
    }
}


