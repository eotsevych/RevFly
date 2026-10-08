use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    resolve_input_device_reporting_fallback(device_name).map(|(device, _)| device)
}

/// Like `resolve_input_device`, but also returns the requested device name when it wasn't found and
/// the system default was used instead (e.g. the chosen USB mic was unplugged).
fn resolve_input_device_reporting_fallback(device_name: Option<&str>) -> Option<(cpal::Device, Option<String>)> {
    let host = cpal::default_host();
    let mut missing = None;
    if let Some(target) = device_name {
        let clean = target.trim();
        if clean != "Default" && clean != "System default" && clean != "System Default" && !clean.is_empty() {
            if let Ok(devs) = host.devices() {
                for d in devs {
                    if let Ok(name) = d.name() {
                        if name.trim() == clean {
                            return Some((d, None));
                        }
                    }
                }
            }
            if let Ok(mut devs) = host.input_devices() {
                if let Some(found) = devs.find(|d| d.name().map(|n| n.trim() == clean).unwrap_or(false)) {
                    return Some((found, None));
                }
            }
            missing = Some(clean.to_string());
        }
    }
    host.default_input_device().map(|d| (d, missing))
}

/// Seconds of recording after which the microphone must have delivered real audio.
pub const MIC_CHECK_AFTER: Duration = Duration::from_millis(2500);
/// Longest Stop waits for the audio captured before the stop to arrive from the callback. It
/// usually arrives within one buffer (~10-20 ms); Bluetooth mics deliver in bigger batches.
const STOP_FLUSH_MAX: Duration = Duration::from_millis(120);

/// When the user asked to stop, and whether the callback has delivered audio from past that moment
/// (so everything before it is in the buffer).
#[derive(Default)]
struct StopMark {
    at: Option<Instant>,
    reached: bool,
}
/// Once audio has flowed, this long without new samples means the device went away.
const MIC_STALL_AFTER: Duration = Duration::from_millis(1500);
/// Once real audio has flowed, this long of pure digital silence means the microphone stopped
/// delivering sound (natural pauses always keep a noise floor).
const MIC_WENT_SILENT_AFTER: Duration = Duration::from_secs(3);
/// Exact zeros are a stronger signal than near-silence: a working microphone never delivers a full
/// second of them. This is what macOS feeds when a Bluetooth headset drops its call-audio (SCO) link.
const MIC_EXACT_ZERO_AFTER: Duration = Duration::from_secs(1);
/// How many times one recording may reopen a microphone that went dead.
const MAX_STREAM_RESTARTS: u32 = 2;
/// A live microphone always has a noise floor above this; below it the OS is feeding zeros.
const MIC_SILENCE_PEAK: f32 = 1e-4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicProblem {
    /// The stream opened but no samples arrived.
    NoAudio,
    /// Samples arrived but they are pure digital silence (muted, or blocked by privacy settings).
    Silent,
    /// Audio flowed, then stopped (typically unplugged mid-recording).
    Stalled,
    /// Real audio flowed, then only digital silence arrived (the device stopped delivering sound).
    WentSilent,
    /// The audio system reported the device as gone.
    Disconnected,
}

impl MicProblem {
    /// Problems found before any usable audio arrived. Those cancel the recording; later ones only
    /// warn, so speech captured before the problem is still transcribed.
    pub fn is_startup(self) -> bool {
        matches!(self, MicProblem::NoAudio | MicProblem::Silent)
    }
}

/// What the health check noticed in the latest audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicEvent {
    Problem(MicProblem),
    /// Audio is flowing again after a mid-recording problem.
    Recovered,
}

/// A microphone failure detected while recording, reported to the controller.
#[derive(Clone, Debug)]
pub struct MicReport {
    pub problem: MicProblem,
    /// Device actually recorded from.
    pub device: String,
    /// The user's chosen device, when it was missing and `device` is the fallback default.
    pub missing_device: Option<String>,
    /// The stream is being reopened to recover from this problem.
    pub reconnecting: bool,
}

/// Decides from the incoming audio whether the microphone works. Fed by the level monitor.
#[derive(Debug)]
struct MicHealth {
    started: Instant,
    samples: usize,
    peak: f32,
    last_audio: Option<Instant>,
    /// Start of the current stretch of digital silence.
    dead_since: Option<Instant>,
    /// Start of the current stretch of exact zeros.
    exact_zero_since: Option<Instant>,
    /// The mid-recording problem currently reported, until audio recovers.
    dropout: Option<MicProblem>,
    /// After a reopen: when to report that the reopened stream is still dead.
    resume_deadline: Option<Instant>,
    checked: bool,
}

impl MicHealth {
    fn new(started: Instant) -> Self {
        Self {
            started,
            samples: 0,
            peak: 0.0,
            last_audio: None,
            dead_since: None,
            exact_zero_since: None,
            dropout: None,
            resume_deadline: None,
            checked: false,
        }
    }

    /// For a stream reopened after `problem`: no startup check (speech was already captured), and
    /// the first real audio reports recovery.
    fn resumed(started: Instant, problem: MicProblem) -> Self {
        Self {
            checked: true,
            dropout: Some(problem),
            resume_deadline: Some(started + MIC_WENT_SILENT_AFTER),
            ..Self::new(started)
        }
    }

    fn observe(&mut self, new_samples: &[f32], now: Instant) -> Option<MicEvent> {
        let has_new = !new_samples.is_empty();
        let all_dead = new_samples.iter().all(|s| !s.is_finite() || s.abs() < MIC_SILENCE_PEAK);
        let all_exact_zero = new_samples.iter().all(|s| *s == 0.0);
        if has_new {
            self.samples += new_samples.len();
            self.peak = new_samples.iter().filter(|s| s.is_finite()).fold(self.peak, |p, s| p.max(s.abs()));
            self.last_audio = Some(now);
            self.dead_since = if all_dead { Some(self.dead_since.unwrap_or(now)) } else { None };
            self.exact_zero_since = if all_exact_zero { Some(self.exact_zero_since.unwrap_or(now)) } else { None };
        }

        if !self.checked {
            if now.duration_since(self.started) < MIC_CHECK_AFTER {
                return None;
            }
            self.checked = true;
            if self.samples == 0 {
                return Some(MicEvent::Problem(MicProblem::NoAudio));
            }
            if self.peak < MIC_SILENCE_PEAK {
                return Some(MicEvent::Problem(MicProblem::Silent));
            }
            return None;
        }

        if let Some(problem) = self.dropout {
            if has_new && !all_dead {
                self.dropout = None;
                self.resume_deadline = None;
                return Some(MicEvent::Recovered);
            }
            // A reopened stream that still delivers nothing: report it again (another reopen, or a warning).
            if self.resume_deadline.is_some_and(|at| now >= at) {
                self.resume_deadline = None;
                return Some(MicEvent::Problem(problem));
            }
            return None;
        }
        let stalled = self.last_audio.is_some_and(|last| now.duration_since(last) >= MIC_STALL_AFTER);
        let went_silent = self.dead_since.is_some_and(|since| now.duration_since(since) >= MIC_WENT_SILENT_AFTER)
            || self.exact_zero_since.is_some_and(|since| now.duration_since(since) >= MIC_EXACT_ZERO_AFTER);
        let problem = if stalled {
            MicProblem::Stalled
        } else if went_silent {
            MicProblem::WentSilent
        } else {
            return None;
        };
        self.dropout = Some(problem);
        Some(MicEvent::Problem(problem))
    }
}

/// Sends a device-gone report to the controller once per recording.
fn report_mic_problem(app_handle: &AppHandle, reported: &AtomicBool, report: MicReport) {
    if reported.swap(true, Ordering::SeqCst) {
        return;
    }
    log::warn!("Microphone health check failed: {:?}", report);
    crate::app_controller::report_mic_problem(app_handle, report);
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
        reply: Sender<Result<StreamInfo, String>>,
    },
    Stop {
        /// Noise reduction mix, None when off.
        denoise: Option<f32>,
        reply: Sender<CapturedAudio>,
    },
    Cancel,
    /// Sent by a stream's own monitor: reopen it because it went dead. Ignored unless `generation`
    /// is still the current stream (a stop or a newer stream wins).
    Reopen { generation: u64, problem: MicProblem },
}

/// One finished recording.
pub struct CapturedAudio {
    /// Leveled 16 kHz mono: what VAD and the speech models get.
    pub speech: Vec<f32>,
    /// Unleveled mono at the microphone's own rate, kept for playback quality.
    native: Vec<f32>,
    native_rate: u32,
    plan: crate::leveler::LevelPlan,
    /// What the microphone delivered, measured before leveling and noise reduction.
    pub stats: CaptureStats,
}

/// The input stream a recording opened.
#[derive(Clone, Debug)]
pub struct StreamInfo {
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// The user's chosen device, when it was missing and `device` is the fallback default.
    pub missing_device: Option<String>,
}

/// Facts about the raw 16 kHz capture, for the stop log: how much audio arrived and whether parts of
/// it were dead (digital silence, invalid values), which shows a microphone that quietly stopped.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CaptureStats {
    pub captured_sec: f32,
    /// Total digital silence, in seconds (a live microphone never produces it).
    pub zero_sec: f32,
    /// Longest stretch of digital silence and where it started, in seconds.
    pub longest_zero_sec: f32,
    pub longest_zero_at_sec: f32,
    /// NaN or infinite samples.
    pub invalid_samples: usize,
}

impl CaptureStats {
    /// Measures 16 kHz mono samples. Silence is counted in 10 ms blocks.
    pub fn measure(samples: &[f32]) -> Self {
        const BLOCK: usize = 160;
        let rate = 16000.0;
        let mut stats = Self { captured_sec: samples.len() as f32 / rate, ..Self::default() };
        stats.invalid_samples = samples.iter().filter(|s| !s.is_finite()).count();
        let (mut run_start, mut run_len) = (0usize, 0usize);
        for (i, block) in samples.chunks(BLOCK).enumerate() {
            let dead = block.iter().all(|s| s.is_finite() && s.abs() < MIC_SILENCE_PEAK);
            if dead {
                if run_len == 0 {
                    run_start = i * BLOCK;
                }
                run_len += block.len();
                stats.zero_sec += block.len() as f32 / rate;
                if run_len as f32 / rate > stats.longest_zero_sec {
                    stats.longest_zero_sec = run_len as f32 / rate;
                    stats.longest_zero_at_sec = run_start as f32 / rate;
                }
            } else {
                run_len = 0;
            }
        }
        stats
    }
}

impl CapturedAudio {
    fn new(speech_raw: Vec<f32>, native: Vec<f32>, native_rate: u32, denoise: Option<f32>) -> Self {
        let (speech_raw, native) = match denoise {
            // Denoise the full-bandwidth copy, then derive the 16 kHz speech from it.
            Some(wet) if !native.is_empty() => {
                let clean = crate::denoise::denoise(&native, native_rate, wet);
                let speech = crate::denoise::resample(&clean, native_rate, 16000).unwrap_or(speech_raw);
                (speech, clean)
            }
            Some(wet) => (crate::denoise::denoise(&speech_raw, 16000, wet), native),
            None => (speech_raw, native),
        };
        let (speech, plan) = crate::leveler::level(&speech_raw, 16000);
        Self { speech, native, native_rate, plan, stats: CaptureStats::default() }
    }

    pub fn empty() -> Self {
        Self { speech: Vec::new(), native: Vec::new(), native_rate: 16000, plan: Default::default(), stats: CaptureStats::default() }
    }

    pub fn level_plan(&self) -> crate::leveler::LevelPlan {
        self.plan
    }

    /// Leveled mono at the microphone's own rate, and that rate, for history playback.
    pub fn playback(&self) -> (Vec<f32>, u32) {
        if self.native.is_empty() {
            return (self.speech.clone(), 16000);
        }
        (crate::leveler::apply(&self.native, self.native_rate, self.plan), self.native_rate)
    }
}

pub struct AudioRecorder {
    cmd_tx: Sender<AudioCmd>,
    is_recording: Arc<AtomicBool>,
    /// Moment the user asked to stop; audio captured after it is dropped.
    stop_at: Arc<Mutex<StopMark>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = channel::<AudioCmd>();
        let is_recording = Arc::new(AtomicBool::new(false));
        let is_rec_thread = Arc::clone(&is_recording);
        let stop_at = Arc::new(Mutex::new(StopMark::default()));
        let stop_at_thread = Arc::clone(&stop_at);
        let reopen_tx = cmd_tx.clone();

        // Dedicated audio recording thread so cpal::Stream stays on one thread
        thread::spawn(move || {
            let mut active_stream: Option<cpal::Stream> = None;
            let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
            let native_buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
            let mut native_rate = 16000;
            // A reopened stream at another rate makes the full-bandwidth copy inconsistent; playback
            // then falls back to the 16 kHz speech audio.
            let mut native_valid = true;
            // Identifies the current stream; bumped by every start, reopen, stop and cancel so a
            // stale monitor or Reopen request is ignored.
            let generation = Arc::new(AtomicU64::new(0));
            let mut current: Option<(AppHandle, Option<String>)> = None;
            let mut restarts = 0u32;

            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    AudioCmd::Start { app_handle, device_name, reply } => {
                        stop_mic_test();
                        // A stream still open here was never stopped; it would keep appending to
                        // the shared buffers alongside the new one and double the audio.
                        if let Some(stale) = active_stream.take() {
                            let _ = stale.pause();
                            log::warn!("Closed a microphone stream left open by an earlier recording");
                        }
                        if let Ok(mut b) = buffer.lock() {
                            b.clear();
                        }
                        if let Ok(mut b) = native_buffer.lock() {
                            b.clear();
                        }
                        if let Ok(mut s) = stop_at_thread.lock() {
                            *s = StopMark::default();
                        }

                        // Mark recording active before starting stream so initial audio buffers are captured immediately
                        is_rec_thread.store(true, Ordering::SeqCst);
                        let open_started = Instant::now();
                        restarts = 0;
                        native_valid = true;
                        current = Some((app_handle.clone(), device_name.clone()));
                        let control = StreamControl {
                            reopen_tx: reopen_tx.clone(),
                            generation: Arc::clone(&generation),
                            id: generation.fetch_add(1, Ordering::SeqCst) + 1,
                            resumed_from: None,
                            can_reopen: true,
                        };

                        let res = start_stream_inner(
                            &mut active_stream,
                            Arc::clone(&buffer),
                            Arc::clone(&native_buffer),
                            Arc::clone(&is_rec_thread),
                            Arc::clone(&stop_at_thread),
                            app_handle,
                            device_name,
                            control,
                        );

                        match &res {
                            Ok(info) => {
                                native_rate = info.sample_rate;
                                log::info!(
                                    "Microphone stream \"{}\" opened in {} ms at {} Hz, {} ch",
                                    info.device,
                                    open_started.elapsed().as_millis(),
                                    info.sample_rate,
                                    info.channels
                                );
                            }
                            Err(_) => is_rec_thread.store(false, Ordering::SeqCst),
                        }
                        let _ = reply.send(res);
                    }
                    AudioCmd::Stop { denoise, reply } => {
                        // Wait for the audio captured before the stop to arrive from the callback. Frames
                        // captured after the stop are discarded there (see frames_before_stop).
                        let flush_started = Instant::now();
                        while flush_started.elapsed() < STOP_FLUSH_MAX
                            && !stop_at_thread.lock().map(|s| s.reached).unwrap_or(true)
                        {
                            thread::sleep(Duration::from_millis(5));
                        }
                        is_rec_thread.store(false, Ordering::SeqCst);
                        generation.fetch_add(1, Ordering::SeqCst);
                        current = None;
                        if let Some(stream) = active_stream.take() {
                            let _ = stream.pause();
                        }
                        let speech = buffer.lock().map(|mut b| std::mem::take(&mut *b)).unwrap_or_default();
                        let stats = CaptureStats::measure(&speech);
                        // At 16 kHz the speech buffer already is the native audio.
                        let native = native_buffer.lock().map(|mut b| std::mem::take(&mut *b)).unwrap_or_default();
                        let native = if native_rate == 16000 || !native_valid { Vec::new() } else { native };
                        let processing_started = Instant::now();
                        let mut captured = CapturedAudio::new(speech, native, native_rate, denoise);
                        captured.stats = stats;
                        let plan = captured.level_plan();
                        log::info!(
                            "Noise reduction {:?}, voice leveling gain {:+.1} dB, make-up {:+.1} dB, in {} ms",
                            denoise,
                            plan.gain_db,
                            plan.makeup_db,
                            processing_started.elapsed().as_millis()
                        );
                        let _ = reply.send(captured);
                    }
                    AudioCmd::Cancel => {
                        is_rec_thread.store(false, Ordering::SeqCst);
                        generation.fetch_add(1, Ordering::SeqCst);
                        current = None;
                        if let Some(stream) = active_stream.take() {
                            let _ = stream.pause();
                        }
                        if let Ok(mut b) = buffer.lock() {
                            b.clear();
                        }
                        if let Ok(mut b) = native_buffer.lock() {
                            b.clear();
                        }
                    }
                    AudioCmd::Reopen { generation: requested, problem } => {
                        let still_current = is_rec_thread.load(Ordering::SeqCst) && requested == generation.load(Ordering::SeqCst);
                        let Some((app_handle, device_name)) = current.clone().filter(|_| still_current) else {
                            continue;
                        };
                        // Closing and reopening the input makes macOS bring back e.g. a Bluetooth
                        // headset's dropped call-audio link. Audio keeps appending to the same buffers.
                        if let Some(stream) = active_stream.take() {
                            let _ = stream.pause();
                        }
                        restarts += 1;
                        let reopen_started = Instant::now();
                        let control = StreamControl {
                            reopen_tx: reopen_tx.clone(),
                            generation: Arc::clone(&generation),
                            id: generation.fetch_add(1, Ordering::SeqCst) + 1,
                            resumed_from: Some(problem),
                            can_reopen: restarts < MAX_STREAM_RESTARTS,
                        };
                        let captured_sec = buffer.lock().map(|b| b.len() as f32 / 16000.0).unwrap_or_default();
                        let res = start_stream_inner(
                            &mut active_stream,
                            Arc::clone(&buffer),
                            Arc::clone(&native_buffer),
                            Arc::clone(&is_rec_thread),
                            Arc::clone(&stop_at_thread),
                            app_handle.clone(),
                            device_name,
                            control,
                        );
                        let data_dir = crate::settings::get_data_dir();
                        match res {
                            Ok(info) => {
                                if info.sample_rate != native_rate {
                                    native_valid = false;
                                }
                                crate::pipeline_logger::log_stage_event(
                                    &data_dir,
                                    "REC_RESTART",
                                    &format!(
                                        "Reopened the microphone after {:?} at {:.1} s of audio (attempt {}/{}): \"{}\" at {} Hz in {} ms",
                                        problem,
                                        captured_sec,
                                        restarts,
                                        MAX_STREAM_RESTARTS,
                                        info.device,
                                        info.sample_rate,
                                        reopen_started.elapsed().as_millis()
                                    ),
                                );
                            }
                            Err(e) => {
                                crate::pipeline_logger::log_stage_event(
                                    &data_dir,
                                    "REC_RESTART_FAILED",
                                    &format!("Could not reopen the microphone after {:?} (attempt {}): {}", problem, restarts, e),
                                );
                                crate::app_controller::report_mic_problem(
                                    &app_handle,
                                    MicReport {
                                        problem: MicProblem::Disconnected,
                                        device: "the microphone".to_string(),
                                        missing_device: None,
                                        reconnecting: false,
                                    },
                                );
                            }
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

    pub fn start_recording(&self, app_handle: AppHandle, device_name: Option<String>) -> Result<StreamInfo, String> {
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

    /// `denoise` is the noise reduction mix (see `denoise::strength_wet`), None when off.
    pub fn stop_recording(&self, denoise: Option<f32>) -> CapturedAudio {
        if let Ok(mut s) = self.stop_at.lock() {
            *s = StopMark { at: Some(Instant::now()), reached: false };
        }
        let (reply_tx, reply_rx) = channel();
        if self.cmd_tx.send(AudioCmd::Stop { denoise, reply: reply_tx }).is_ok() {
            reply_rx.recv().unwrap_or_else(|_| CapturedAudio::empty())
        } else {
            CapturedAudio::empty()
        }
    }

    pub fn cancel_recording(&self) {
        let _ = self.cmd_tx.send(AudioCmd::Cancel);
    }
}

/// How a stream's monitor may ask the recorder thread to reopen it.
struct StreamControl {
    reopen_tx: Sender<AudioCmd>,
    /// The recorder's current stream generation; this stream stops monitoring once it moves on.
    generation: Arc<AtomicU64>,
    /// This stream's generation.
    id: u64,
    /// Set when this stream replaces one that went dead.
    resumed_from: Option<MicProblem>,
    /// Whether another reopen is allowed for this recording.
    can_reopen: bool,
}

#[allow(clippy::too_many_arguments)]
fn start_stream_inner(
    active_stream: &mut Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    native_buffer: Arc<Mutex<Vec<f32>>>,
    is_rec: Arc<AtomicBool>,
    stop_at: Arc<Mutex<StopMark>>,
    app_handle: AppHandle,
    device_name: Option<String>,
    control: StreamControl,
) -> Result<StreamInfo, String> {
    let (device, missing_device) = resolve_input_device_reporting_fallback(device_name.as_deref())
        .ok_or_else(|| "no microphone is connected".to_string())?;
    let device_label = device.name().unwrap_or_else(|_| "Default microphone".to_string());
    if let Some(missing) = &missing_device {
        log::warn!("Microphone \"{}\" not found, recording from default \"{}\"", missing, device_label);
    }
    // One report per recording, whether it comes from the stream error callback or the level monitor.
    let reported = Arc::new(AtomicBool::new(false));

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
    let stream_info = StreamInfo {
        device: device_label.clone(),
        sample_rate,
        channels: channels as u16,
        missing_device: missing_device.clone(),
    };

    let resampler_state = Arc::new(Mutex::new(ResamplerState::new()));
    let resampler_clone = Arc::clone(&resampler_state);
    let buffer_clone = Arc::clone(&buffer);
    let native_clone = Arc::clone(&native_buffer);
    let is_rec_cb = Arc::clone(&is_rec);
    let stop_at_cb = Arc::clone(&stop_at);

    let err_app = app_handle.clone();
    let err_reported = Arc::clone(&reported);
    let err_rec = Arc::clone(&is_rec);
    let err_reopen_tx = control.reopen_tx.clone();
    let err_generation = Arc::clone(&control.generation);
    let (err_id, err_can_reopen) = (control.id, control.can_reopen);
    let err_report = MicReport {
        problem: MicProblem::Disconnected,
        device: device_label.clone(),
        missing_device: missing_device.clone(),
        reconnecting: err_can_reopen,
    };
    let err_fn = move |err: cpal::StreamError| {
        log::error!("Audio stream error: {}", err);
        let is_current = err_generation.load(Ordering::SeqCst) == err_id;
        if matches!(err, cpal::StreamError::DeviceNotAvailable) && err_rec.load(Ordering::Relaxed) && is_current {
            report_mic_problem(&err_app, &err_reported, err_report.clone());
            if err_can_reopen {
                // Reopen picks the chosen mic again, or the system default if it's gone for good.
                let _ = err_reopen_tx.send(AudioCmd::Reopen { generation: err_id, problem: MicProblem::Disconnected });
            }
        }
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
                    process_input_samples(data, channels, sample_rate, &resampler_cb, &buffer_clone, &native_clone);
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
                    process_input_samples(&floats, channels, sample_rate, &resampler_cb, &buffer_clone, &native_clone);
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

    // Monitoring thread for the pill's equalizer (loudness plus pitch-band energy of the latest audio)
    // and the microphone health check: real audio must arrive within MIC_CHECK_AFTER and keep coming.
    let is_rec_monitor = Arc::clone(&is_rec);
    let buf_monitor = Arc::clone(&buffer);
    thread::spawn(move || {
        let analyzer = SpectrumAnalyzer::new();
        let mut window = vec![0.0f32; SPECTRUM_WINDOW];
        // A reopened stream appends to the existing buffer: only audio from here on is this stream's.
        let mut last_len = buf_monitor.lock().map(|b| b.len()).unwrap_or(0);
        let mut health = match control.resumed_from {
            Some(problem) => MicHealth::resumed(Instant::now(), problem),
            None => MicHealth::new(Instant::now()),
        };
        let is_current = || control.generation.load(Ordering::SeqCst) == control.id;

        while is_rec_monitor.load(Ordering::Relaxed) && is_current() {
            thread::sleep(Duration::from_millis(40));

            let (has_new_audio, problem) = match buf_monitor.lock() {
                Ok(b) => {
                    let new_samples = &b[last_len.min(b.len())..];
                    let problem = health.observe(new_samples, Instant::now());
                    let has_new = b.len() > last_len;
                    if has_new {
                        last_len = b.len();
                        let n = b.len().min(SPECTRUM_WINDOW);
                        window.fill(0.0);
                        window[SPECTRUM_WINDOW - n..].copy_from_slice(&b[b.len() - n..]);
                    }
                    (has_new, problem)
                }
                Err(_) => (false, None),
            };

            match problem {
                Some(MicEvent::Problem(problem)) if is_rec_monitor.load(Ordering::Relaxed) && is_current() => {
                    // A microphone that died mid-recording is reopened (up to MAX_STREAM_RESTARTS);
                    // problems before any audio arrived cancel the recording instead.
                    let reopen = !problem.is_startup() && control.can_reopen;
                    let report = MicReport {
                        problem,
                        device: device_label.clone(),
                        missing_device: missing_device.clone(),
                        reconnecting: reopen,
                    };
                    log::warn!("Microphone health check: {:?}", report);
                    crate::app_controller::report_mic_problem(&app_handle, report);
                    if reopen {
                        let _ = control.reopen_tx.send(AudioCmd::Reopen { generation: control.id, problem });
                        break; // the replacement stream gets its own monitor
                    }
                    if problem.is_startup() {
                        break; // the recording is cancelled
                    }
                }
                Some(MicEvent::Recovered) if is_rec_monitor.load(Ordering::Relaxed) => {
                    log::info!("Microphone audio recovered on \"{}\"", device_label);
                    // A reopened stream that landed on the system default because the chosen mic is gone.
                    let switched_to = missing_device.as_ref().map(|_| device_label.clone());
                    crate::app_controller::report_mic_recovered(&app_handle, switched_to);
                }
                _ => {}
            }

            let levels = if has_new_audio { analyzer.analyze(&window) } else { AudioLevels::silent() };
            let _ = app_handle.emit("audio-level", levels);
        }
    });

    Ok(stream_info)
}

/// Number of leading frames in this callback that were captured before the user pressed stop.
/// Returns all frames while recording; once stop is requested, frames captured afterwards
/// (including the stop earcon picked up by the mic) are cut off.
fn frames_before_stop(
    info: &cpal::InputCallbackInfo,
    frames: usize,
    sample_rate: u32,
    stop_at: &Mutex<StopMark>,
) -> usize {
    let Ok(mut mark) = stop_at.lock() else {
        return frames;
    };
    let Some(stop) = mark.at else {
        return frames;
    };
    let ts = info.timestamp();
    let latency = ts.callback.duration_since(&ts.capture).unwrap_or_default();
    let Some(first_capture) = Instant::now().checked_sub(latency) else {
        mark.reached = true;
        return frames;
    };
    if stop <= first_capture {
        mark.reached = true;
        return 0;
    }
    let captured_before = (stop - first_capture).as_secs_f64() * sample_rate as f64;
    let keep = (captured_before as usize).min(frames);
    if keep < frames {
        mark.reached = true;
    }
    keep
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
    native_buffer: &Arc<Mutex<Vec<f32>>>,
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

    if src_rate != 16000 {
        if let Ok(mut native) = native_buffer.lock() {
            native.extend_from_slice(&mono_chunk);
        }
    }
    resample_mono_to_16k(&mono_chunk, src_rate, resampler, target_buffer);
}

/// Resamples a whole mono recording to 16 kHz, e.g. a saved history file for the Lab.
pub fn resample_to_16k(samples: &[f32], src_rate: u32) -> Vec<f32> {
    if src_rate == 16000 {
        return samples.to_vec();
    }
    let resampler = Arc::new(Mutex::new(ResamplerState::new()));
    let target = Arc::new(Mutex::new(Vec::with_capacity(samples.len() * 16000 / src_rate.max(1) as usize)));
    resample_mono_to_16k(samples, src_rate, &resampler, &target);
    let out = target.lock().map(|mut t| std::mem::take(&mut *t)).unwrap_or_default();
    out
}

fn resample_mono_to_16k(
    mono_chunk: &[f32],
    src_rate: u32,
    resampler: &Arc<Mutex<ResamplerState>>,
    target_buffer: &Arc<Mutex<Vec<f32>>>,
) {
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

    fn ms(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn mic_health_waits_for_the_check_window() {
        let t0 = Instant::now();
        let mut h = MicHealth::new(t0);
        assert_eq!(h.observe(&[], ms(t0, 1000)), None);
        assert_eq!(h.observe(&[], ms(t0, 2400)), None);
    }

    #[test]
    fn mic_health_flags_no_audio_and_silence() {
        let t0 = Instant::now();
        let mut none = MicHealth::new(t0);
        assert_eq!(none.observe(&[], ms(t0, 2600)), Some(MicEvent::Problem(MicProblem::NoAudio)));

        let mut zeros = MicHealth::new(t0);
        assert_eq!(zeros.observe(&[0.0; 1600], ms(t0, 500)), None);
        assert_eq!(zeros.observe(&[0.0; 1600], ms(t0, 2600)), Some(MicEvent::Problem(MicProblem::Silent)));
    }

    #[test]
    fn mic_health_passes_a_live_mic_then_flags_a_stall() {
        let t0 = Instant::now();
        let mut h = MicHealth::new(t0);
        let noise = [0.003f32, -0.002, 0.004, -0.001];
        assert_eq!(h.observe(&noise, ms(t0, 500)), None);
        assert_eq!(h.observe(&noise, ms(t0, 2600)), None, "live mic passes the check");
        assert_eq!(h.observe(&[], ms(t0, 3500)), None, "short gap is fine");
        assert_eq!(h.observe(&[], ms(t0, 4200)), Some(MicEvent::Problem(MicProblem::Stalled)));
        assert_eq!(h.observe(&[], ms(t0, 5000)), None, "reported once");
        assert_eq!(h.observe(&noise, ms(t0, 5100)), Some(MicEvent::Recovered));
    }

    #[test]
    fn mic_health_flags_audio_that_turns_into_digital_silence() {
        let t0 = Instant::now();
        let mut h = MicHealth::new(t0);
        let voice = [0.2f32, -0.15, 0.1, -0.05];
        assert_eq!(h.observe(&voice, ms(t0, 1000)), None);
        assert_eq!(h.observe(&voice, ms(t0, 2600)), None);
        // Samples keep arriving, but they are all exact zeros (a dropped Bluetooth call-audio link).
        assert_eq!(h.observe(&[0.0; 640], ms(t0, 3000)), None);
        assert_eq!(h.observe(&[0.0; 640], ms(t0, 3800)), None, "under 1 s of exact zeros");
        assert_eq!(h.observe(&[0.0; 640], ms(t0, 4100)), Some(MicEvent::Problem(MicProblem::WentSilent)));
        assert_eq!(h.observe(&[0.0; 640], ms(t0, 4500)), None, "reported once");
        assert_eq!(h.observe(&voice, ms(t0, 5000)), Some(MicEvent::Recovered));
    }

    #[test]
    fn mic_health_needs_3_s_of_near_silence_that_is_not_exact_zeros() {
        let t0 = Instant::now();
        let mut h = MicHealth::new(t0);
        let voice = [0.2f32, -0.15, 0.1, -0.05];
        let faint = [0.00002f32, -0.00001, 0.00003, -0.00002];
        assert_eq!(h.observe(&voice, ms(t0, 2600)), None);
        assert_eq!(h.observe(&faint, ms(t0, 3000)), None);
        assert_eq!(h.observe(&faint, ms(t0, 5500)), None, "near-silence under 3 s");
        assert_eq!(h.observe(&faint, ms(t0, 6100)), Some(MicEvent::Problem(MicProblem::WentSilent)));
    }

    #[test]
    fn reopened_stream_recovers_on_first_real_audio_or_reports_again() {
        let t0 = Instant::now();
        let voice = [0.2f32, -0.15, 0.1, -0.05];
        let mut back = MicHealth::resumed(t0, MicProblem::WentSilent);
        assert_eq!(back.observe(&[0.0; 640], ms(t0, 400)), None, "no startup check after a reopen");
        assert_eq!(back.observe(&voice, ms(t0, 700)), Some(MicEvent::Recovered));

        let mut still_dead = MicHealth::resumed(t0, MicProblem::WentSilent);
        assert_eq!(still_dead.observe(&[0.0; 640], ms(t0, 2900)), None);
        assert_eq!(still_dead.observe(&[0.0; 640], ms(t0, 3100)), Some(MicEvent::Problem(MicProblem::WentSilent)));
        assert_eq!(still_dead.observe(&[0.0; 640], ms(t0, 3500)), None, "reported once");
    }

    #[test]
    fn mic_health_ignores_quiet_but_live_pauses() {
        let t0 = Instant::now();
        let mut h = MicHealth::new(t0);
        let room_tone = [0.0004f32, -0.0003, 0.0005, -0.0002];
        for step in 1..=200 {
            assert_eq!(h.observe(&room_tone, ms(t0, step * 50)), None);
        }
    }

    #[test]
    fn capture_stats_find_the_dead_stretch() {
        // 2 s of voice, then 3 s of zeros, then 1 s of voice, with two invalid samples.
        let mut samples: Vec<f32> = (0..32000).map(|i| (i as f32 * 0.05).sin() * 0.2).collect();
        samples.extend(std::iter::repeat(0.0).take(48000));
        samples.extend((0..16000).map(|i| (i as f32 * 0.05).sin() * 0.2));
        samples[100] = f32::NAN;
        samples[200] = f32::INFINITY;
        let stats = CaptureStats::measure(&samples);
        assert!((stats.captured_sec - 6.0).abs() < 1e-3);
        assert!((stats.zero_sec - 3.0).abs() < 0.02, "{stats:?}");
        assert!((stats.longest_zero_sec - 3.0).abs() < 0.02, "{stats:?}");
        assert!((stats.longest_zero_at_sec - 2.0).abs() < 0.02, "{stats:?}");
        assert_eq!(stats.invalid_samples, 2);
    }

    #[test]
    fn silence_has_no_band_energy() {
        let levels = SpectrumAnalyzer::new().analyze(&vec![0.0; SPECTRUM_WINDOW]);
        assert_eq!(levels.volume, 0.0);
        assert_eq!(levels.bands, [0.0; 7]);
    }

    #[test]
    fn denoised_recording_keeps_length_and_level() {
        let native: Vec<f32> = (0..48000 * 2)
            .map(|i| {
                let t = i as f32 / 48000.0;
                let phase = 2.0 * std::f32::consts::PI * (150.0 * t + 60.0 * t * t);
                0.02 * (phase.sin() + 0.5 * (3.0 * phase).sin())
            })
            .collect();
        let speech_raw = resample_to_16k(&native, 48000);
        let captured = CapturedAudio::new(speech_raw.clone(), native.clone(), 48000, Some(0.9));

        assert!((captured.speech.len() as i64 - speech_raw.len() as i64).abs() <= 32, "{} vs {}", captured.speech.len(), speech_raw.len());
        let (playback, rate) = captured.playback();
        assert_eq!((playback.len(), rate), (native.len(), 48000));
        let level = crate::leveler::speech_level_db(&captured.speech, 16000).unwrap();
        assert!((level + 18.0).abs() < 2.0, "speech level {level}");
    }

    #[test]
    fn test_sinc_resampler_48k_to_16k() {
        let resampler = Arc::new(Mutex::new(ResamplerState::new()));
        let target = Arc::new(Mutex::new(Vec::new()));
        let native = Arc::new(Mutex::new(Vec::new()));

        // Generate 4800 samples at 48000 Hz = 0.1 second of 1000 Hz tone
        let mut input = Vec::with_capacity(4800);
        for i in 0..4800 {
            let t = i as f32 / 48000.0;
            input.push((2.0 * std::f32::consts::PI * 1000.0 * t).sin());
        }

        // Feed in 3 chunks to test streaming continuity
        for chunk in input.chunks(1600) {
            process_input_samples(chunk, 1, 48000, &resampler, &target, &native);
        }

        assert_eq!(native.lock().unwrap().len(), 4800, "native-rate copy keeps every input sample");
        let output = target.lock().unwrap().clone();
        // 0.1 sec at 16000 Hz is ~1600 samples (give or take filter margin)
        assert!(output.len() >= 1550 && output.len() <= 1620, "Output length: {}", output.len());
        for &sample in &output {
            assert!(!sample.is_nan());
            assert!(sample.abs() <= 1.2);
        }
    }
}


