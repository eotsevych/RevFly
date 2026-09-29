use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
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
    /// Moment the user asked to stop; audio captured after it is dropped.
    stop_at: Arc<Mutex<Option<Instant>>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = channel::<AudioCmd>();
        let is_recording = Arc::new(AtomicBool::new(false));
        let is_rec_thread = Arc::clone(&is_recording);
        let stop_at = Arc::new(Mutex::new(None::<Instant>));
        let stop_at_thread = Arc::clone(&stop_at);

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
                        if let Ok(mut s) = stop_at_thread.lock() {
                            *s = None;
                        }

                        // Mark recording active before starting stream so initial audio buffers are captured immediately
                        is_rec_thread.store(true, Ordering::SeqCst);
                        let open_started = Instant::now();

                        let res = start_stream_inner(
                            &mut active_stream,
                            Arc::clone(&buffer),
                            Arc::clone(&is_rec_thread),
                            Arc::clone(&stop_at_thread),
                            app_handle,
                            device_name,
                        );

                        if res.is_err() {
                            is_rec_thread.store(false, Ordering::SeqCst);
                        } else {
                            log::info!("Microphone stream opened in {} ms", open_started.elapsed().as_millis());
                        }
                        let _ = reply.send(res);
                    }
                    AudioCmd::Stop { reply } => {
                        // Allow a brief flush window for audio captured before the stop to arrive from the callback.
                        // Frames captured after the stop are discarded in the callback (see frames_before_stop).
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
            stop_at,
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
        if let Ok(mut s) = self.stop_at.lock() {
            *s = Some(Instant::now());
        }
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
    stop_at: Arc<Mutex<Option<Instant>>>,
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
    let stop_at_cb = Arc::clone(&stop_at);

    let err_fn = |err| {
        log::error!("Audio stream error: {}", err);
    };

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            let resampler_cb = Arc::clone(&resampler_clone);
            device.build_input_stream(
                &supported_config.into(),
                move |data: &[f32], info: &cpal::InputCallbackInfo| {
                    if !is_rec_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    let keep = frames_before_stop(info, data.len() / channels.max(1), sample_rate, &stop_at_cb);
                    let data = &data[..keep * channels];
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
                move |data: &[i16], info: &cpal::InputCallbackInfo| {
                    if !is_rec_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    let keep = frames_before_stop(info, data.len() / channels.max(1), sample_rate, &stop_at_cb);
                    let floats: Vec<f32> = data[..keep * channels].iter().map(|&s| s.to_float_sample()).collect();
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

    // Monitoring thread for the pill's equalizer: loudness plus pitch-band energy of the latest audio
    let is_rec_monitor = Arc::clone(&is_rec);
    let buf_monitor = Arc::clone(&buffer);
    thread::spawn(move || {
        let analyzer = SpectrumAnalyzer::new();
        let mut window = vec![0.0f32; SPECTRUM_WINDOW];
        let mut last_len = 0;

        while is_rec_monitor.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(40));

            let has_new_audio = match buf_monitor.lock() {
                Ok(b) if b.len() > last_len => {
                    last_len = b.len();
                    let n = b.len().min(SPECTRUM_WINDOW);
                    window.fill(0.0);
                    window[SPECTRUM_WINDOW - n..].copy_from_slice(&b[b.len() - n..]);
                    true
                }
                _ => false,
            };

            let levels = if has_new_audio { analyzer.analyze(&window) } else { AudioLevels::silent() };
            let _ = app_handle.emit("audio-level", levels);
        }
    });

    Ok(())
}

/// Number of leading frames in this callback that were captured before the user pressed stop.
/// Returns all frames while recording; once stop is requested, frames captured afterwards
/// (including the stop earcon picked up by the mic) are cut off.
fn frames_before_stop(
    info: &cpal::InputCallbackInfo,
    frames: usize,
    sample_rate: u32,
    stop_at: &Mutex<Option<Instant>>,
) -> usize {
    let Some(stop) = stop_at.lock().ok().and_then(|s| *s) else {
        return frames;
    };
    let ts = info.timestamp();
    let latency = ts.callback.duration_since(&ts.capture).unwrap_or_default();
    let Some(first_capture) = Instant::now().checked_sub(latency) else {
        return frames;
    };
    if stop <= first_capture {
        return 0;
    }
    let captured_before = (stop - first_capture).as_secs_f64() * sample_rate as f64;
    (captured_before as usize).min(frames)
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

/// Samples per spectrum frame: 512 at 16 kHz = 32 ms, 31.25 Hz per frequency bin.
const SPECTRUM_WINDOW: usize = 512;
const SPECTRUM_RATE: f32 = 16000.0;
/// Pitch bands, low to high: voice fundamentals, vowels, consonants, sibilants and breath.
const PITCH_BANDS_HZ: [(f32, f32); 4] = [(100.0, 350.0), (350.0, 1200.0), (1200.0, 3500.0), (3500.0, 7000.0)];
/// Speech energy falls off with frequency, so higher bands get a boost (dB) to register visibly.
const BAND_TILT_DB: [f32; 4] = [0.0, 5.0, 10.0, 14.0];
/// A band this many dB below the loudest band reads as zero.
const BAND_RANGE_DB: f32 = 24.0;
/// Below this loudness the pill treats the input as silence.
const SILENCE_VOLUME: f32 = 0.02;

/// Payload of the `audio-level` event that drives the pill's equalizer.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AudioLevels {
    /// Overall loudness, 0..1.
    pub volume: f32,
    /// Pitch energy per bar, 0..1, mirrored from the centre:
    /// [high, high-mid, mid, low, mid, high-mid, high].
    pub bands: [f32; 7],
}

impl AudioLevels {
    pub fn silent() -> Self {
        Self { volume: 0.0, bands: [0.0; 7] }
    }
}

/// Splits short audio frames into pitch bands with a Hann-windowed DFT over the bins of interest.
struct SpectrumAnalyzer {
    hann: Vec<f32>,
    cos: Vec<f32>,
    sin: Vec<f32>,
    band_bins: [(usize, usize); 4],
}

impl SpectrumAnalyzer {
    fn new() -> Self {
        let n = SPECTRUM_WINDOW;
        let tau = 2.0 * std::f32::consts::PI;
        let hann = (0..n).map(|i| 0.5 - 0.5 * (tau * i as f32 / (n - 1) as f32).cos()).collect();
        let cos = (0..n).map(|i| (tau * i as f32 / n as f32).cos()).collect();
        let sin = (0..n).map(|i| (tau * i as f32 / n as f32).sin()).collect();
        let bin_hz = SPECTRUM_RATE / n as f32;
        let band_bins = PITCH_BANDS_HZ.map(|(lo, hi)| ((lo / bin_hz).ceil() as usize, (hi / bin_hz).floor() as usize));
        Self { hann, cos, sin, band_bins }
    }

    fn analyze(&self, frame: &[f32]) -> AudioLevels {
        let n = SPECTRUM_WINDOW;
        debug_assert_eq!(frame.len(), n);
        let rms = (frame.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        // Same speech sensitivity curve the pill has always used.
        let volume = (rms * 14.0).clamp(0.0, 1.0);
        if volume < SILENCE_VOLUME {
            return AudioLevels { volume, bands: [0.0; 7] };
        }

        let windowed: Vec<f32> = frame.iter().zip(&self.hann).map(|(x, w)| x * w).collect();
        let band_db = self.band_bins.map(|(lo, hi)| {
            let mut power = 0.0f32;
            for k in lo..=hi {
                let (mut re, mut im) = (0.0f32, 0.0f32);
                for (t, &x) in windowed.iter().enumerate() {
                    let idx = (k * t) % n;
                    re += x * self.cos[idx];
                    im -= x * self.sin[idx];
                }
                power += re * re + im * im;
            }
            10.0 * (power / (hi - lo + 1) as f32 + 1e-12).log10()
        });

        let tilted: Vec<f32> = band_db.iter().zip(BAND_TILT_DB).map(|(db, tilt)| db + tilt).collect();
        let loudest = tilted.iter().cloned().fold(f32::MIN, f32::max);
        let energy: Vec<f32> = tilted
            .iter()
            .map(|db| ((db - (loudest - BAND_RANGE_DB)) / BAND_RANGE_DB).clamp(0.0, 1.0))
            .collect();
        let [low, mid, high_mid, high] = [energy[0], energy[1], energy[2], energy[3]];
        AudioLevels { volume, bands: [high, high_mid, mid, low, mid, high_mid, high] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, amplitude: f32) -> Vec<f32> {
        (0..SPECTRUM_WINDOW)
            .map(|i| amplitude * (2.0 * std::f32::consts::PI * hz * i as f32 / SPECTRUM_RATE).sin())
            .collect()
    }

    #[test]
    fn low_tone_lifts_the_centre_bar() {
        let levels = SpectrumAnalyzer::new().analyze(&tone(200.0, 0.2));
        assert!(levels.volume > 0.5);
        assert_eq!(levels.bands[3], 1.0, "{:?}", levels.bands);
        assert!(levels.bands[0] < 0.5, "{:?}", levels.bands);
        assert_eq!(levels.bands[2], levels.bands[4], "bars mirror around the centre");
    }

    #[test]
    fn high_tone_lifts_the_edge_bars() {
        let levels = SpectrumAnalyzer::new().analyze(&tone(5000.0, 0.2));
        assert_eq!(levels.bands[0], 1.0, "{:?}", levels.bands);
        assert_eq!(levels.bands[6], 1.0, "{:?}", levels.bands);
        assert!(levels.bands[3] < 0.5, "{:?}", levels.bands);
    }

    #[test]
    fn silence_has_no_band_energy() {
        let levels = SpectrumAnalyzer::new().analyze(&vec![0.0; SPECTRUM_WINDOW]);
        assert_eq!(levels.volume, 0.0);
        assert_eq!(levels.bands, [0.0; 7]);
    }

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


