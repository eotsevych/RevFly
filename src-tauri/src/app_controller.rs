use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::AudioRecorder;
use crate::history::{HistoryManager, HistoryRecord};
use crate::paste::copy_and_paste;
use crate::pipeline_logger::{current_timestamp, log_stage_event, PipelineLogManager, TranscriptionDiagnosticLog};
use crate::settings::{get_data_dir, AppSettings};
use crate::transcribe::Transcriber;
use crate::translate::{normalize_lang, should_skip};
use crate::vad::trim_silence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantPhase {
    Idle,
    Listening,
    Transcribing,
    Translating,
    Done,
    Error,
}

impl AssistantPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            AssistantPhase::Idle => "idle",
            AssistantPhase::Listening => "listening",
            AssistantPhase::Transcribing => "transcribing",
            AssistantPhase::Translating => "translating",
            AssistantPhase::Done => "done",
            AssistantPhase::Error => "error",
        }
    }
}

fn hide_main_window(app_handle: &AppHandle) {
    let app = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        crate::fullscreen_pill::release();
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.hide();
        }
    });
}

fn get_active_monitor(win: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
    let monitors = win.available_monitors().ok()?;
    if monitors.is_empty() {
        return None;
    }
    if monitors.len() == 1 {
        return monitors.into_iter().next();
    }

    #[cfg(target_os = "macos")]
    unsafe {
        use std::ffi::{c_char, c_void, CStr};
        #[link(name = "AppKit", kind = "framework")]
        extern "C" {
            fn objc_msgSend(receiver: *mut c_void, sel: *const c_void, ...) -> *mut c_void;
            fn sel_registerName(str: *const u8) -> *const c_void;
            fn objc_getClass(str: *const u8) -> *mut c_void;
        }

        #[repr(C)]
        #[derive(Copy, Clone, Debug)]
        struct NSPoint {
            x: f64,
            y: f64,
        }
        #[repr(C)]
        #[derive(Copy, Clone, Debug)]
        struct NSSize {
            width: f64,
            height: f64,
        }
        #[repr(C)]
        #[derive(Copy, Clone, Debug)]
        struct NSRect {
            origin: NSPoint,
            size: NSSize,
        }

        let nsevent_cls = objc_getClass(b"NSEvent\0".as_ptr());
        let nsscreen_cls = objc_getClass(b"NSScreen\0".as_ptr());

        if !nsevent_cls.is_null() && !nsscreen_cls.is_null() {
            let sel_mouse = sel_registerName(b"mouseLocation\0".as_ptr());
            let mouse_fn: extern "C" fn(*mut c_void, *const c_void) -> NSPoint =
                std::mem::transmute(objc_msgSend as *const ());
            let mouse = mouse_fn(nsevent_cls, sel_mouse);

            let sel_screens = sel_registerName(b"screens\0".as_ptr());
            let sel_count = sel_registerName(b"count\0".as_ptr());
            let sel_object_at_index = sel_registerName(b"objectAtIndex:\0".as_ptr());
            let sel_frame = sel_registerName(b"frame\0".as_ptr());
            let sel_name = sel_registerName(b"localizedName\0".as_ptr());
            let sel_utf8 = sel_registerName(b"UTF8String\0".as_ptr());

            let screen_array = objc_msgSend(nsscreen_cls, sel_screens);
            if !screen_array.is_null() {
                let count = objc_msgSend(screen_array, sel_count) as usize;
                let frame_fn: extern "C" fn(*mut c_void, *const c_void) -> NSRect =
                    std::mem::transmute(objc_msgSend as *const ());

                for i in 0..count {
                    let screen = objc_msgSend(screen_array, sel_object_at_index, i);
                    if !screen.is_null() {
                        let frame = frame_fn(screen, sel_frame);
                        let in_rect = mouse.x >= frame.origin.x
                            && mouse.x <= (frame.origin.x + frame.size.width)
                            && mouse.y >= frame.origin.y
                            && mouse.y <= (frame.origin.y + frame.size.height);

                        if in_rect {
                            let ns_name = objc_msgSend(screen, sel_name);
                            if !ns_name.is_null() {
                                let c_str_ptr = objc_msgSend(ns_name, sel_utf8) as *const c_char;
                                if !c_str_ptr.is_null() {
                                    if let Ok(name_str) = CStr::from_ptr(c_str_ptr).to_str() {
                                        if let Some(m) = monitors.iter().find(|m| m.name().map(|n| n == name_str).unwrap_or(false)) {
                                            return Some(m.clone());
                                        }
                                    }
                                }
                            }
                            if i < monitors.len() {
                                return Some(monitors[i].clone());
                            }
                        }
                    }
                }
            }

            // Fallback: NSScreen.mainScreen
            let sel_main = sel_registerName(b"mainScreen\0".as_ptr());
            let main_screen = objc_msgSend(nsscreen_cls, sel_main);
            if !main_screen.is_null() {
                let ns_name = objc_msgSend(main_screen, sel_name);
                if !ns_name.is_null() {
                    let c_str_ptr = objc_msgSend(ns_name, sel_utf8) as *const c_char;
                    if !c_str_ptr.is_null() {
                        if let Ok(name_str) = CStr::from_ptr(c_str_ptr).to_str() {
                            if let Some(m) = monitors.iter().find(|m| m.name().map(|n| n == name_str).unwrap_or(false)) {
                                return Some(m.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    if let Ok(cursor) = win.cursor_position() {
        for m in &monitors {
            let pos = m.position();
            let size = m.size();
            if cursor.x >= pos.x as f64
                && cursor.x <= (pos.x + size.width as i32) as f64
                && cursor.y >= pos.y as f64
                && cursor.y <= (pos.y + size.height as i32) as f64
            {
                return Some(m.clone());
            }
        }
    }

    win.current_monitor().ok().flatten().or_else(|| win.primary_monitor().ok().flatten())
}

fn show_main_window(app_handle: &AppHandle, settings: &AppSettings) {
    let app = app_handle.clone();
    let (win_x, win_y) = (settings.window_x, settings.window_y);
    let _ = app_handle.run_on_main_thread(move || {
        if let Some(win) = app.get_webview_window("main") {
            // A previous long error may have enlarged the pill window.
            let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize { width: PILL_WINDOW.0, height: PILL_WINDOW.1 }));
            let active_monitor = get_active_monitor(&win);
            let m_pos = active_monitor.as_ref().map(|m| m.position().clone()).unwrap_or(tauri::PhysicalPosition { x: 0, y: 0 });
            let m_size = active_monitor.as_ref().map(|m| m.size().clone()).unwrap_or(tauri::PhysicalSize { width: 1920, height: 1080 });
            let scale = active_monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);

            let pill_w = (340.0 * scale).round() as i32;
            let pill_h = (100.0 * scale).round() as i32;

            // Check if saved position is inside the active monitor bounds
            let is_on_active_monitor = if let (Some(x), Some(y)) = (win_x, win_y) {
                x >= m_pos.x && (x + 100) <= (m_pos.x + m_size.width as i32)
                    && y >= m_pos.y && (y + 50) <= (m_pos.y + m_size.height as i32)
            } else {
                false
            };

            if is_on_active_monitor {
                if let (Some(x), Some(y)) = (win_x, win_y) {
                    let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
                }
            } else {
                let target_x = m_pos.x + (m_size.width as i32 - pill_w) / 2;
                let target_y = if let Some(y) = win_y {
                    if let Ok(monitors) = win.available_monitors() {
                        let other_mon = monitors.iter().find(|m| {
                            let p = m.position();
                            let s = m.size();
                            y >= p.y && y <= (p.y + s.height as i32)
                        });
                        if let Some(om) = other_mon {
                            let rel_y = (y - om.position().y) as f64 / om.size().height as f64;
                            m_pos.y + (rel_y * m_size.height as f64) as i32
                        } else {
                            m_pos.y + (m_size.height as i32 - pill_h) / 2
                        }
                    } else {
                        m_pos.y + (m_size.height as i32 - pill_h) / 2
                    }
                } else {
                    m_pos.y + (m_size.height as i32 - pill_h) / 2
                };

                let clamped_x = target_x.max(m_pos.x).min(m_pos.x + m_size.width as i32 - pill_w);
                let clamped_y = target_y.max(m_pos.y).min(m_pos.y + m_size.height as i32 - pill_h);
                let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x: clamped_x, y: clamped_y }));
            }

            let _ = win.show();
            let _ = win.set_shadow(false);
            apply_pill_window_behavior(&win, true);
        }
    });
}

/// Makes the voice pill float above full-screen apps on every Space. The pill never takes keyboard
/// focus (so the paste lands in the app the user was typing in) because the window is created with
/// `"focusable": false` in tauri.conf.json. Don't toggle that at runtime: on Windows it rewrites the
/// window's extended style, which can leave the transparent WebView2 pill unpainted.
///
/// The window must stay the NSWindow subclass tao created: swapping its class (e.g. to NSPanel) breaks
/// WebKit's KVO observers and aborts the app on macOS 27 when the view hierarchy is rebuilt.
pub(crate) fn apply_pill_window_behavior(win: &tauri::WebviewWindow, order_front: bool) {
    #[cfg(not(target_os = "macos"))]
    let _ = (win, order_front);

    #[cfg(target_os = "macos")]
    unsafe {
        use std::ffi::c_void;
        #[link(name = "AppKit", kind = "framework")]
        extern "C" {
            fn objc_msgSend(receiver: *mut c_void, sel: *const c_void, ...) -> *mut c_void;
            fn sel_registerName(str: *const u8) -> *const c_void;
        }

        // objc_msgSend must be called through a non-variadic signature: on Apple Silicon variadic
        // arguments go on the stack, but the callee reads them from registers.
        let send_usize: extern "C" fn(*mut c_void, *const c_void, usize) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_isize: extern "C" fn(*mut c_void, *const c_void, isize) =
            std::mem::transmute(objc_msgSend as *const ());

        if let Ok(ns_win) = win.ns_window() {
            let ptr = ns_win as *mut c_void;

            // NSWindowCollectionBehaviorCanJoinAllSpaces (1) | IgnoresCycle (64) | FullScreenAuxiliary (256)
            let behavior: usize = (1 << 0) | (1 << 6) | (1 << 8);
            send_usize(ptr, sel_registerName(b"setCollectionBehavior:\0".as_ptr()), behavior);

            // NSScreenSaverWindowLevel (1000) floats directly above full-screen and active apps
            send_isize(ptr, sel_registerName(b"setLevel:\0".as_ptr()), 1000);

            if order_front {
                // A native full-screen app's Space never composites this window no matter the
                // collection behavior above, so the pill is shown from an NSPanel instead (see
                // fullscreen_pill.rs).
                crate::fullscreen_pill::adopt(win);
            }
        } else {
            log::warn!("Pill window: ns_window() failed, could not apply full-screen collection behavior");
        }
    }
}

/// Longest a translation request may take before it counts as failed.
const TRANSLATION_TIMEOUT: Duration = Duration::from_secs(20);

/// The translation service a recording goes to, resolved from settings. Shared by the pipeline and
/// the "Retry translation" action so both send text the same way.
struct TranslationRoute {
    is_custom: bool,
    endpoint: String,
    key: String,
    model: String,
}

impl TranslationRoute {
    fn from_settings(settings: &AppSettings) -> Self {
        let is_custom = settings.translation_provider.to_lowercase().contains("custom");
        let endpoint = if !settings.llm_endpoint.trim().is_empty() {
            settings.llm_endpoint.trim().to_string()
        } else if !settings.local_llm_url.trim().is_empty() {
            settings.local_llm_url.trim().to_string()
        } else {
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions".to_string()
        };
        let key = if !settings.llm_api_key.trim().is_empty() {
            settings.llm_api_key.trim().to_string()
        } else if !settings.api_key.trim().is_empty() {
            settings.api_key.trim().to_string()
        } else {
            settings.local_llm_api_key.trim().to_string()
        };
        let model = if is_custom {
            if settings.custom_api_model.trim().is_empty() {
                "gpt-4o-mini".to_string()
            } else {
                settings.custom_api_model.trim().to_string()
            }
        } else if !settings.llm_model.trim().is_empty() {
            settings.llm_model.trim().to_string()
        } else if !settings.gemini_model.trim().is_empty() {
            settings.gemini_model.trim().to_string()
        } else if !settings.local_llm_model.trim().is_empty() {
            settings.local_llm_model.trim().to_string()
        } else {
            "gemini-3.6-flash".to_string()
        };
        Self { is_custom, endpoint, key, model }
    }

    fn label(&self) -> &'static str {
        if self.is_custom { "Custom API" } else { "LLM" }
    }

    /// One-line description for logs, e.g. `LLM gemini-3.6-flash at https://…`.
    fn describe(&self, settings: &AppSettings) -> String {
        let url = if self.is_custom { settings.custom_api_url.trim() } else { self.endpoint.as_str() };
        format!("{} {} at {}", self.label(), self.model, url)
    }

    async fn translate(&self, settings: &AppSettings, text: &str) -> Result<String, String> {
        let request = async {
            if self.is_custom {
                crate::translate::translate_with_openai_compatible(
                    text,
                    &settings.custom_api_url,
                    &settings.custom_api_key,
                    &settings.source_lang,
                    &settings.target_lang,
                    &self.model,
                    &settings.prompt_template,
                )
                .await
            } else if self.endpoint.contains("generativelanguage.googleapis.com") && !self.endpoint.contains("/openai") {
                crate::translate::translate_with_gemini(
                    text,
                    &self.key,
                    &settings.source_lang,
                    &settings.target_lang,
                    &self.model,
                    &settings.prompt_template,
                )
                .await
            } else {
                crate::translate::translate_with_openai_compatible(
                    text,
                    &self.endpoint,
                    &self.key,
                    &settings.source_lang,
                    &settings.target_lang,
                    &self.model,
                    &settings.prompt_template,
                )
                .await
            }
        };
        match tokio::time::timeout(TRANSLATION_TIMEOUT, request).await {
            Ok(result) => result,
            Err(_) => Err(format!("timed out after {} s", TRANSLATION_TIMEOUT.as_secs())),
        }
    }
}

/// A translation that failed, kept so the user can retry it from the pill or the tray menu.
#[derive(Clone)]
pub(crate) struct PendingRetry {
    /// Text that was sent for translation (already normalized and masked).
    text: String,
    from_lang: String,
    history_id: i64,
}

/// Entry point for the audio thread; routes a microphone problem to the running controller.
pub fn report_mic_problem(app_handle: &AppHandle, report: crate::audio::MicReport) {
    if let Some(state) = app_handle.try_state::<crate::AppState>() {
        state.controller.on_mic_problem(report);
    }
}

/// Entry point for the audio thread; audio is flowing again after a mid-recording dropout.
pub fn report_mic_recovered(app_handle: &AppHandle, switched_to: Option<String>) {
    if let Some(state) = app_handle.try_state::<crate::AppState>() {
        state.controller.on_mic_recovered(switched_to);
    }
}

/// When and how the current recording started.
pub(crate) struct RecordingStart {
    at: Instant,
    trigger: &'static str,
}

/// Captured audio shorter than the wall-clock recording by more than this means audio was dropped.
const CAPTURE_GAP_WARN_SEC: f32 = 0.5;

/// Logs how a recording ended and whether the audio matches it: wall-clock time between start and
/// stop against the audio actually captured, plus dead (digital silence) or invalid stretches.
fn log_recording_stop(recording_start: &Mutex<Option<RecordingStart>>, trigger: &str, stats: &crate::audio::CaptureStats) {
    let data_dir = get_data_dir();
    let Some(start) = recording_start.lock().unwrap().take() else {
        log_stage_event(&data_dir, "REC_STOP", &format!("Recording stopped by {}; captured {:.2} s (start time unknown)", trigger, stats.captured_sec));
        return;
    };
    let wall = start.at.elapsed().as_secs_f32();
    let missing = wall - stats.captured_sec;
    let dead = if stats.zero_sec >= 0.05 {
        format!(
            "; digital silence {:.2} s (longest {:.2} s starting at {:.2} s)",
            stats.zero_sec, stats.longest_zero_sec, stats.longest_zero_at_sec
        )
    } else {
        "; no digital silence".to_string()
    };
    let invalid = if stats.invalid_samples > 0 { format!("; {} invalid samples", stats.invalid_samples) } else { String::new() };
    log_stage_event(
        &data_dir,
        "REC_STOP",
        &format!(
            "Recording stopped by {} (started by {}): {:.2} s between start and stop, {:.2} s of audio captured ({:+.2} s){}{}",
            trigger, start.trigger, wall, stats.captured_sec, -missing, dead, invalid
        ),
    );
    if missing > CAPTURE_GAP_WARN_SEC {
        log_stage_event(&data_dir, "REC_GAP", &format!("{:.2} s of the recording never arrived from the microphone", missing));
        log::warn!("Recording lost {:.2} s of audio ({:.2} s wall, {:.2} s captured)", missing, wall, stats.captured_sec);
    }
    if stats.longest_zero_sec >= 1.0 || stats.invalid_samples > 0 {
        log::warn!("Recording had dead audio: {:?}", stats);
    }
}

/// Pill title and text for a microphone problem that started after real audio arrived.
fn mic_dropout_message(report: &crate::audio::MicReport) -> (&'static str, String) {
    use crate::audio::MicProblem;
    let device = &report.device;
    if report.reconnecting {
        return (
            "Reconnecting Mic…",
            format!("\"{device}\" stopped sending sound. Reopening it; what you said so far is kept."),
        );
    }
    match report.problem {
        MicProblem::Disconnected => ("Mic Disconnected", format!("\"{device}\" was disconnected. Press the hotkey to finish; what you said so far is kept.")),
        MicProblem::Stalled => ("Mic Stopped", format!("No audio from \"{device}\" for 2 s. Press the hotkey to finish; what you said so far is kept.")),
        _ => ("Mic Went Silent", format!("\"{device}\" sends only silence. Press the hotkey to finish; what you said so far is kept.")),
    }
}

/// Where the user fixes the microphone choice.
const MIC_SETTING: &str = "Settings → General → Microphone input";
/// How long an error that needs reading stays on screen.
const LONG_ERROR_HOLD: Duration = Duration::from_secs(9);
/// Pill window size while it shows a long error, and its normal size.
const LONG_ERROR_WINDOW: (f64, f64) = (400.0, 150.0);
const PILL_WINDOW: (f64, f64) = (340.0, 100.0);

/// Title and explanation shown in the pill for a microphone problem.
fn mic_problem_message(report: &crate::audio::MicReport) -> (&'static str, String) {
    use crate::audio::MicProblem;
    let device = &report.device;
    if let Some(missing) = &report.missing_device {
        let what = match report.problem {
            MicProblem::NoAudio | MicProblem::Silent => "sends no audio",
            MicProblem::Stalled | MicProblem::WentSilent | MicProblem::Disconnected => "stopped sending audio",
        };
        return (
            "Microphone Not Found",
            format!("\"{missing}\" isn't connected, and the default \"{device}\" {what}. Pick a working mic in {MIC_SETTING}."),
        );
    }
    match report.problem {
        MicProblem::NoAudio => (
            "Microphone Not Working",
            format!("No audio from \"{device}\" in the first 2 seconds. Check it's connected, or pick another mic in {MIC_SETTING}."),
        ),
        MicProblem::Silent => (
            "Microphone Is Silent",
            format!("\"{device}\" sends only silence. It may be muted, or mic access is blocked in privacy settings. Check {MIC_SETTING}."),
        ),
        MicProblem::Stalled | MicProblem::WentSilent => (
            "Microphone Stopped",
            format!("\"{device}\" stopped sending audio, maybe it was unplugged. Check {MIC_SETTING}."),
        ),
        MicProblem::Disconnected => (
            "Microphone Disconnected",
            format!("\"{device}\" was disconnected while recording. Reconnect it or pick another mic in {MIC_SETTING}."),
        ),
    }
}

fn set_pill_window_size(app_handle: &AppHandle, (width, height): (f64, f64)) {
    let app = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
            crate::fullscreen_pill::sync_frame();
        }
    });
}

/// Shows an error the user needs to read in a larger pill for `LONG_ERROR_HOLD`, then hides it,
/// unless a new recording (a new session) has started meanwhile.
fn show_long_error(app_handle: &AppHandle, session_id: &Arc<AtomicU64>, session: u64, title: &str, subtitle: &str) {
    show_long_error_payload(
        app_handle,
        session_id,
        session,
        serde_json::json!({ "state": "error", "title": title, "subtitle": subtitle, "long": true }),
    );
}

/// `show_long_error` with a full pill state payload (e.g. one that offers a Retry button).
fn show_long_error_payload(app_handle: &AppHandle, session_id: &Arc<AtomicU64>, session: u64, payload: serde_json::Value) {
    set_pill_window_size(app_handle, LONG_ERROR_WINDOW);
    let _ = app_handle.emit("assistant-state-changed", payload);
    let app = app_handle.clone();
    let session_id = Arc::clone(session_id);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(LONG_ERROR_HOLD).await;
        if session_id.load(Ordering::SeqCst) != session {
            return;
        }
        let _ = app.emit(
            "assistant-state-changed",
            serde_json::json!({ "state": "idle", "title": "Ready", "subtitle": null }),
        );
        hide_main_window(&app);
        set_pill_window_size(&app, PILL_WINDOW);
    });
}

fn mask_config_from(settings: &AppSettings) -> crate::masker::MaskConfig {
    crate::masker::MaskConfig {
        enabled: settings.mask_confidential,
        rules: crate::masker::parse_rules(&settings.mask_words, &settings.mask_format),
        threshold: settings.mask_threshold,
        default_mask: settings.mask_format.clone(),
        mask_emails: settings.mask_emails,
        mask_phones: settings.mask_phones,
        mask_cards: settings.mask_cards,
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// True when a recording carries no signal at all. A live microphone always has some noise floor, so
/// this means the OS blocked or muted the input (e.g. Windows microphone privacy settings).
/// Simulated chunk events, or none when chunking is off and the recording went to the model whole.
fn chunk_diagnostics(
    chunking: bool,
    total_audio_sec: f32,
    vad_trimmed_sec: f32,
    silence_removed_sec: f32,
    raw_samples_count: usize,
) -> (Vec<crate::pipeline_logger::ChunkDiagnosticEvent>, Vec<String>) {
    if chunking {
        crate::pipeline_logger::generate_chunk_diagnostics(total_audio_sec, vad_trimmed_sec, silence_removed_sec, raw_samples_count)
    } else {
        let actions = crate::pipeline_logger::whole_track_diagnostics(total_audio_sec, vad_trimmed_sec, silence_removed_sec, raw_samples_count);
        (Vec::new(), actions)
    }
}

/// Time allowed for one transcription: 30 s plus half the audio length, so long recordings fit.
fn inference_timeout(audio_sec: f32) -> Duration {
    Duration::from_secs_f32(30.0 + audio_sec.max(0.0) * 0.5)
}

/// Keeps a recording whose transcription failed: saves it to history and adds a Logs entry
/// pointing at the whole recording, so it can be played back and retried with Second Try.
#[allow(clippy::too_many_arguments)]
fn keep_failed_recording(
    history: &HistoryManager,
    logger: &PipelineLogManager,
    app_handle: &AppHandle,
    captured: &crate::audio::CapturedAudio,
    settings: &AppSettings,
    vad: &crate::vad::VadResult,
    duration_sec: f32,
    data_dir: &std::path::Path,
    session_audio: &Option<String>,
    pipeline_ms: u64,
    reason: &str,
) {
    let marker = format!("[{}]", reason);
    let _ = history.add_entry(
        &marker,
        &marker,
        "unknown",
        &settings.target_lang,
        duration_sec,
        Some(captured),
        &settings.storage_mode,
        settings.storage_cap_mb,
    );
    let (chunks, mut actions) = chunk_diagnostics(
        settings.audio_chunking,
        duration_sec,
        vad.trimmed_duration_sec,
        vad.silence_removed_sec,
        captured.speech.len(),
    );
    actions.push(format!("[FAILED] {}", reason));
    let log = TranscriptionDiagnosticLog {
        id: logger.next_id(),
        timestamp: current_timestamp(),
        audio_duration_sec: duration_sec,
        audio_samples_count: captured.speech.len(),
        vad_trim_ms: vad.duration_ms,
        vad_original_sec: vad.original_duration_sec,
        vad_trimmed_sec: vad.trimmed_duration_sec,
        vad_silence_removed_sec: vad.silence_removed_sec,
        model_name: settings.model_name.clone(),
        gpu_metal_active: Transcriber::is_metal_supported(),
        threads_count: 0,
        whisper_inference_ms: 0,
        whisper_speed_factor: 0.0,
        detected_lang: "unknown".to_string(),
        raw_text: marker,
        translation_skipped: true,
        translation_skip_reason: "Transcription failed".to_string(),
        translation_ms: 0,
        translation_error: None,
        final_text: String::new(),
        clipboard_paste_ms: 0,
        history_save_ms: 0,
        total_pipeline_ms: pipeline_ms,
        audio_filename: session_audio.clone(),
        vad_audio_filename: Some("latest_vad_trimmed.wav".to_string()),
        whisper_raw_output: Some(reason.to_string()),
        segments_count: 0,
        chunk_events: chunks,
        whole_track: !settings.audio_chunking,
        action_logs: actions,
    };
    logger.add_log(log.clone(), data_dir);
    let _ = app_handle.emit("transcription-diagnostic-log", &log);
}

/// Whole recording of this session for the log's playback and Second Try; none in private mode.
fn session_audio_filename(data_dir: &std::path::Path, storage_mode: &str, audio: &crate::audio::CapturedAudio) -> Option<String> {
    let private = storage_mode == "private" || storage_mode == "private_mode";
    let saved = if private { None } else { crate::pipeline_logger::save_session_audio(data_dir, audio) };
    saved.or_else(|| Some("latest_recording.wav".to_string()))
}

fn is_digital_silence(samples: &[f32]) -> bool {
    samples.iter().all(|s| s.abs() < 1e-4)
}

/// Returns the model path for a recording that's waiting on it. If the model still has to download,
/// the pill shows the download progress and is put back to "Transcribing…" afterwards.
async fn ensure_model_for_recording(
    app_handle: &AppHandle,
    model_name: &str,
    transcribing_subtitle: &str,
) -> Result<std::path::PathBuf, String> {
    if crate::model_download::is_model_present(model_name) {
        return crate::model_download::ensure(app_handle, model_name).await;
    }
    let pill_wait = crate::model_download::PillWait::start(app_handle);
    let result = crate::model_download::ensure(app_handle, model_name).await;
    drop(pill_wait);
    let _ = app_handle.emit(
        "assistant-state-changed",
        serde_json::json!({
            "state": "transcribing",
            "title": "Transcribing…",
            "subtitle": transcribing_subtitle
        }),
    );
    result
}

pub struct AppController {
    app_handle: AppHandle,
    phase: Arc<Mutex<AssistantPhase>>,
    recorder: Arc<AudioRecorder>,
    transcriber: Arc<Mutex<Transcriber>>,
    parakeet_transcriber: Arc<Mutex<crate::parakeet::ParakeetTranscriber>>,
    history: Arc<HistoryManager>,
    settings: Arc<Mutex<AppSettings>>,
    pipeline_logger: Arc<PipelineLogManager>,
    last_activity: Arc<Mutex<Instant>>,
    session_id: Arc<AtomicU64>,
    /// The last failed translation, offered for retry until another recording finishes.
    pending_retry: Arc<Mutex<Option<PendingRetry>>>,
    /// When and how the current recording started, for the start/stop log.
    recording_start: Arc<Mutex<Option<RecordingStart>>>,
}

impl AppController {
    pub fn new(app_handle: AppHandle) -> Result<Self, String> {
        let settings = AppSettings::load();
        let history = Arc::new(HistoryManager::new()?);

        // Run retention policy on startup
        let _ = history.enforce_retention_policy(settings.retention_days);

        let phase = Arc::new(Mutex::new(AssistantPhase::Idle));
        let recorder = Arc::new(AudioRecorder::new());
        let transcriber = Arc::new(Mutex::new(Transcriber::new()));
        let parakeet_transcriber = Arc::new(Mutex::new(crate::parakeet::ParakeetTranscriber::new()));
        let settings_arc = Arc::new(Mutex::new(settings));
        let pipeline_logger = Arc::new(PipelineLogManager::new());
        let last_activity = Arc::new(Mutex::new(Instant::now()));
        let session_id = Arc::new(AtomicU64::new(0));

        // Background task: Configurable idle model unloader (drops RAM to ~45 MB)
        {
            let transcriber_idle = Arc::clone(&transcriber);
            let parakeet_idle = Arc::clone(&parakeet_transcriber);
            let phase_idle = Arc::clone(&phase);
            let settings_idle = Arc::clone(&settings_arc);
            let last_act_idle = Arc::clone(&last_activity);
            let data_dir_idle = get_data_dir();
            let app_handle_idle = app_handle.clone();

            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    let idle_limit_sec = {
                        let s = settings_idle.lock().unwrap();
                        s.model_idle_unload_sec
                    };
                    if idle_limit_sec == 0 {
                        continue; // 0 = Never unload
                    }
                    let is_idle = {
                        let p = phase_idle.lock().unwrap();
                        *p == AssistantPhase::Idle
                    };
                    if !is_idle {
                        continue;
                    }
                    let elapsed = {
                        let t = last_act_idle.lock().unwrap();
                        t.elapsed().as_secs()
                    };
                    if elapsed >= idle_limit_sec {
                        let mut tr_unloaded = false;
                        let mut pk_unloaded = false;
                        {
                            let mut tr = transcriber_idle.lock().unwrap();
                            if tr.has_context() {
                                tr.unload();
                                tr_unloaded = true;
                            }
                        }
                        {
                            let mut pk = parakeet_idle.lock().unwrap();
                            if pk.has_model() {
                                pk.unload();
                                pk_unloaded = true;
                            }
                        }
                        if tr_unloaded || pk_unloaded {
                            crate::tray::update_tray_model_status(&app_handle_idle, false);
                            log_stage_event(
                                &data_dir_idle,
                                "IDLE_CLEANUP",
                                &format!(
                                    "Freed speech model from RAM after {}s idle (limit: {}s)",
                                    elapsed, idle_limit_sec
                                ),
                            );
                        }
                    }
                }
            });
        }

        Ok(Self {
            app_handle,
            phase,
            recorder,
            transcriber,
            parakeet_transcriber,
            history,
            settings: settings_arc,
            pipeline_logger,
            last_activity,
            session_id,
            pending_retry: Arc::new(Mutex::new(None)),
            recording_start: Arc::new(Mutex::new(None)),
        })
    }

    pub fn get_phase(&self) -> String {
        self.phase.lock().unwrap().as_str().to_string()
    }

    pub fn is_active(&self) -> bool {
        let p = self.phase.lock().unwrap();
        *p != AssistantPhase::Idle
    }

    pub fn is_listening(&self) -> bool {
        let p = self.phase.lock().unwrap();
        *p == AssistantPhase::Listening
    }

    pub fn get_settings(&self) -> AppSettings {
        self.settings.lock().unwrap().clone()
    }

    pub fn save_settings(&self, new_settings: AppSettings) -> Result<(), String> {
        let old_model = { self.settings.lock().unwrap().model_name.clone() };
        new_settings.save()?;
        {
            let mut s = self.settings.lock().unwrap();
            *s = new_settings.clone();
        }

        // Single Model Policy: If active model changed, drop the other from RAM immediately
        if old_model != new_settings.model_name {
            if new_settings.model_name == "parakeet-tdt-0.6b-v3" {
                let mut tr = self.transcriber.lock().unwrap();
                tr.unload();
            } else {
                let mut pk = self.parakeet_transcriber.lock().unwrap();
                pk.unload();
            }
        }

        Ok(())
    }

    pub fn get_history(&self, limit: usize) -> Result<Vec<HistoryRecord>, String> {
        self.history.get_records(limit)
    }

    pub fn clear_history(&self) -> Result<(), String> {
        self.history.clear_history()?;
        self.pipeline_logger.clear_logs();
        Ok(())
    }

    pub fn get_diagnostic_logs(&self, limit: usize) -> Vec<TranscriptionDiagnosticLog> {
        self.pipeline_logger.get_logs(limit)
    }

    pub fn clear_diagnostic_logs(&self) {
        self.pipeline_logger.clear_logs();
    }

    pub fn is_model_loaded(&self) -> bool {
        let tr_loaded = self.transcriber.lock().map(|t| t.has_context()).unwrap_or(false);
        let pk_loaded = self.parakeet_transcriber.lock().map(|p| p.has_model()).unwrap_or(false);
        tr_loaded || pk_loaded
    }

    pub fn eject_model(&self) {
        let mut did = false;
        if let Ok(mut t) = self.transcriber.lock() {
            if t.has_context() { t.unload(); did = true; }
        }
        if let Ok(mut pk) = self.parakeet_transcriber.lock() {
            if pk.has_model() { pk.unload(); did = true; }
        }
        if did {
            crate::tray::update_tray_model_status(&self.app_handle, false);
            crate::settings::get_data_dir(); // ensure data dir exists for log
            crate::pipeline_logger::log_stage_event(&crate::settings::get_data_dir(), "EJECT_MODEL", "User ejected LLM from RAM via Control Center");
        }
    }

    pub fn get_audio_data(&self, filename: Option<String>) -> Result<String, String> {
        use base64::Engine;
        let data_dir = get_data_dir();
        let path = if let Some(ref f) = filename {
            if f.is_empty() || f == "latest" || f == "latest_recording.wav" {
                data_dir.join("latest_recording.wav")
            } else {
                let p = self.history.get_audio_file_path(f);
                if p.exists() {
                    p
                } else {
                    data_dir.join(f)
                }
            }
        } else {
            data_dir.join("latest_recording.wav")
        };

        if !path.exists() {
            return Err(format!("Audio file not found: {:?}", path));
        }

        let bytes = std::fs::read(&path)
            .map_err(|e| format!("Failed to read audio file: {}", e))?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(b64)
    }

    pub fn open_audio_folder(&self) -> Result<(), String> {
        let dir = self.history.get_audio_dir();
        crate::custom_models::open_in_file_manager(&dir)
    }

    pub fn play_recorded_audio(&self, filename: Option<String>) -> Result<(), String> {
        let data_dir = get_data_dir();
        let path = if let Some(ref f) = filename {
            if f.is_empty() || f == "latest" || f == "latest_recording.wav" {
                data_dir.join("latest_recording.wav")
            } else {
                let p = self.history.get_audio_file_path(f);
                if p.exists() {
                    p
                } else {
                    data_dir.join(f)
                }
            }
        } else {
            data_dir.join("latest_recording.wav")
        };

        if !path.exists() {
            return Err(format!("Audio file not found: {:?}", path));
        }

        crate::sound::play_wav_file(&path)
    }

    pub fn list_lab_audio(&self) -> Vec<crate::lab::LabAudioItem> {
        crate::lab::list_available_audio_files()
    }

    pub async fn run_lab_experiment(
        &self,
        req: crate::lab::LabExperimentRequest,
    ) -> Result<crate::lab::LabExperimentResult, String> {
        crate::lab::run_experiment(
            &self.app_handle,
            Arc::clone(&self.transcriber),
            Arc::clone(&self.parakeet_transcriber),
            req,
        )
        .await
    }

    pub fn save_lab_custom_audio(&self, base64_wav: String) -> Result<String, String> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&base64_wav)
            .map_err(|e| format!("Base64 decode error: {}", e))?;
        let p = get_data_dir().join("lab_custom.wav");
        std::fs::write(&p, bytes).map_err(|e| format!("Failed to write custom audio: {}", e))?;
        Ok("lab_custom.wav".to_string())
    }

    pub fn cancel(&self, trigger: &'static str) {
        if let Some(start) = self.recording_start.lock().unwrap().take() {
            log_stage_event(
                &get_data_dir(),
                "REC_CANCEL",
                &format!(
                    "Recording cancelled by {} after {:.2} s (started by {})",
                    trigger,
                    start.at.elapsed().as_secs_f32(),
                    start.trigger
                ),
            );
        }
        self.session_id.fetch_add(1, Ordering::SeqCst);
        {
            let mut phase = self.phase.lock().unwrap();
            *phase = AssistantPhase::Idle;
        }
        crate::tray::set_tray_recording(&self.app_handle, false, self.is_model_loaded());
        self.recorder.cancel_recording();

        let _ = self.app_handle.emit(
            "assistant-state-changed",
            serde_json::json!({
                "state": "idle",
                "title": "Ready",
                "subtitle": null
            }),
        );

        hide_main_window(&self.app_handle);
    }

    /// Called from the audio thread when the microphone health check fails during a recording.
    /// Stops the recording (it has no usable audio) and shows what is wrong, long enough to read.
    pub fn on_mic_problem(&self, report: crate::audio::MicReport) {
        if !report.problem.is_startup() {
            self.on_mic_dropout(report);
            return;
        }
        {
            let mut phase = self.phase.lock().unwrap();
            if *phase != AssistantPhase::Listening {
                return;
            }
            // Idle, not Error: pressing the hotkey again starts a fresh recording right away.
            *phase = AssistantPhase::Idle;
        }
        let session = self.session_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.recorder.cancel_recording();
        crate::tray::set_tray_recording(&self.app_handle, false, self.is_model_loaded());

        let (title, subtitle) = mic_problem_message(&report);
        log_stage_event(&get_data_dir(), "MIC_CHECK", &format!("{}: {} ({:?})", title, subtitle, report));
        if let Some(start) = self.recording_start.lock().unwrap().take() {
            log_stage_event(
                &get_data_dir(),
                "REC_CANCEL",
                &format!("Recording cancelled by the mic check after {:.2} s (started by {})", start.at.elapsed().as_secs_f32(), start.trigger),
            );
        }
        show_long_error(&self.app_handle, &self.session_id, session, title, &subtitle);
    }

    /// The microphone failed after real audio arrived. Keep recording, so what was said before is still
    /// transcribed, and warn in the pill until audio comes back or the user stops.
    fn on_mic_dropout(&self, report: crate::audio::MicReport) {
        if *self.phase.lock().unwrap() != AssistantPhase::Listening {
            return;
        }
        let at = self
            .recording_start
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.at.elapsed().as_secs_f32())
            .unwrap_or_default();
        let (title, subtitle) = mic_dropout_message(&report);
        log_stage_event(
            &get_data_dir(),
            "MIC_DROPOUT",
            &format!("{} at {:.1} s into the recording: {} ({:?})", title, at, subtitle, report),
        );
        // "reconnecting" while the stream is being reopened, "down" once reopening didn't help.
        let mic = if report.reconnecting { "reconnecting" } else { "down" };
        set_pill_window_size(&self.app_handle, LONG_ERROR_WINDOW);
        let _ = self.app_handle.emit(
            "assistant-state-changed",
            serde_json::json!({ "state": "listening", "title": title, "subtitle": subtitle, "warning": true, "mic": mic, "long": true }),
        );
    }

    /// Audio is flowing again after a mid-recording dropout: clear the pill's warning. `switched_to`
    /// names the device now in use when the chosen one was gone and the system default took over.
    pub fn on_mic_recovered(&self, switched_to: Option<String>) {
        if *self.phase.lock().unwrap() != AssistantPhase::Listening {
            return;
        }
        let at = self
            .recording_start
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.at.elapsed().as_secs_f32())
            .unwrap_or_default();
        let via = switched_to.as_ref().map(|d| format!(" on \"{}\"", d)).unwrap_or_default();
        log_stage_event(&get_data_dir(), "MIC_RECOVERED", &format!("Microphone audio came back{} at {:.1} s into the recording", via, at));
        // Say which mic took over when it changed, in the larger pill so the name fits.
        let subtitle = switched_to.map(|d| format!("Switched to \"{}\"", d));
        let long = subtitle.is_some();
        set_pill_window_size(&self.app_handle, if long { LONG_ERROR_WINDOW } else { PILL_WINDOW });
        let _ = self.app_handle.emit(
            "assistant-state-changed",
            serde_json::json!({ "state": "listening", "title": "Listening…", "subtitle": subtitle, "long": long }),
        );
    }

    pub fn has_pending_retry(&self) -> bool {
        self.pending_retry.lock().unwrap().is_some()
    }

    /// Translates the last failed text again, with the current settings (so a fixed API key or
    /// endpoint takes effect), pastes the result and updates its history entry.
    pub fn retry_translation(&self) -> Result<(), String> {
        let pending = self
            .pending_retry
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "There is no failed translation to retry".to_string())?;
        {
            let mut phase = self.phase.lock().unwrap();
            if *phase != AssistantPhase::Idle {
                return Err("RevFly is busy; try again when it finishes".to_string());
            }
            *phase = AssistantPhase::Translating;
        }
        // A new session also stops the error pill's hide timer.
        let session = self.session_id.fetch_add(1, Ordering::SeqCst) + 1;
        let settings = self.get_settings();
        let route = TranslationRoute::from_settings(&settings);
        let from_name = crate::translate::format_lang_name(&pending.from_lang);
        let to_name = crate::translate::format_lang_name(&settings.target_lang);

        show_main_window(&self.app_handle, &settings);
        let _ = self.app_handle.emit(
            "assistant-state-changed",
            serde_json::json!({
                "state": "translating",
                "title": "Retrying translation…",
                "subtitle": format!("{} → {} ({})", from_name, to_name, route.label())
            }),
        );

        let app_handle = self.app_handle.clone();
        let phase_arc = Arc::clone(&self.phase);
        let session_id_arc = Arc::clone(&self.session_id);
        let pending_retry_arc = Arc::clone(&self.pending_retry);
        let history_arc = Arc::clone(&self.history);
        tauri::async_runtime::spawn(async move {
            let data_dir = get_data_dir();
            log_stage_event(
                &data_dir,
                "TRANSLATE_RETRY_START",
                &format!("Retrying {} → {} with {}: \"{}\"", from_name, to_name, route.describe(&settings), pending.text),
            );
            let started = Instant::now();
            let result = route.translate(&settings, &pending.text).await;
            let ms = started.elapsed().as_millis() as u64;
            if session_id_arc.load(Ordering::SeqCst) != session {
                log_stage_event(&data_dir, "TRANSLATE_RETRY_CANCELLED", "A new recording started; retry result dropped");
                return;
            }

            match result {
                Ok(translated) => {
                    let mask_config = mask_config_from(&settings);
                    let final_text = if mask_config.enabled {
                        crate::masker::apply_masking(&translated, &mask_config)
                    } else {
                        translated
                    };
                    log_stage_event(&data_dir, "TRANSLATE_RETRY_DONE", &format!("Translated in {} ms: \"{}\"", ms, final_text));
                    *phase_arc.lock().unwrap() = AssistantPhase::Done;
                    if settings.auto_paste {
                        let _ = copy_and_paste(&final_text);
                    } else {
                        let _ = crate::paste::copy_to_clipboard(&final_text);
                    }
                    if let Err(e) = history_arc.update_translation(pending.history_id, &final_text) {
                        log::warn!("Could not update history with the retried translation: {}", e);
                    }
                    *pending_retry_arc.lock().unwrap() = None;
                    crate::tray::set_retry_item(false);
                    let _ = app_handle.emit(
                        "assistant-state-changed",
                        serde_json::json!({
                            "state": "done",
                            "title": "Copied to clipboard",
                            "subtitle": null,
                            "text": final_text
                        }),
                    );
                    tokio::time::sleep(Duration::from_millis(600)).await;
                    let mut p = phase_arc.lock().unwrap();
                    if *p == AssistantPhase::Done && session_id_arc.load(Ordering::SeqCst) == session {
                        *p = AssistantPhase::Idle;
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({ "state": "idle", "title": "Ready", "subtitle": null }),
                        );
                        hide_main_window(&app_handle);
                    }
                }
                Err(e) => {
                    let detail = format!("{} → {} via {} failed again after {} ms: {}", from_name, to_name, route.describe(&settings), ms, e);
                    log_stage_event(&data_dir, "TRANSLATE_RETRY_ERROR", &detail);
                    log::error!("Translation retry failed: {}", detail);
                    *phase_arc.lock().unwrap() = AssistantPhase::Idle;
                    show_long_error_payload(
                        &app_handle,
                        &session_id_arc,
                        session,
                        serde_json::json!({
                            "state": "error",
                            "title": "Translation Failed Again",
                            "subtitle": format!("{}. Check your translation settings, then retry.", capitalize(&e)),
                            "long": true,
                            "retry": true
                        }),
                    );
                }
            }
        });
        Ok(())
    }

    /// Starts or stops a recording. `trigger` says what asked for it (e.g. "hotkey press"); it goes
    /// into the start/stop log.
    pub fn toggle_recording(&self, trigger: &'static str) -> Result<(), String> {
        let cur_phase = { self.phase.lock().unwrap().clone() };

        match cur_phase {
            AssistantPhase::Idle => self.start_listening(trigger),
            AssistantPhase::Listening => self.stop_listening_and_process(trigger),
            _ => {
                // If currently processing or in Done state, ignore toggle or cancel
                Ok(())
            }
        }
    }

    fn start_listening(&self, trigger: &'static str) -> Result<(), String> {
        self.session_id.fetch_add(1, Ordering::SeqCst);
        {
            let mut phase = self.phase.lock().unwrap();
            *phase = AssistantPhase::Listening;
        }

        crate::tray::set_tray_recording(&self.app_handle, true, self.is_model_loaded());

        {
            let mut t = self.last_activity.lock().unwrap();
            *t = Instant::now();
        }

        let settings = self.get_settings();

        // 1. Immediately emit state so the pill UI displays "Listening..." without delay
        let _ = self.app_handle.emit(
            "assistant-state-changed",
            serde_json::json!({
                "state": "listening",
                "title": "Listening…",
                "subtitle": null
            }),
        );

        // 2. Immediately show the pill window on screen
        show_main_window(&self.app_handle, &settings);

        // 3. Play the earcon and wait for it to finish before opening the microphone. Some combo
        // USB/Bluetooth headsets stall for up to a second when playback and capture are opened on
        // the device at nearly the same instant, which otherwise leaks the chime into the start of
        // the recording (see sound::play_sound_blocking).
        if settings.sound_effect {
            crate::sound::play_sound_blocking(crate::sound::AppSound::StartRecording);
        }

        // 4. Start recording on the dedicated audio thread
        let stream = self.recorder.start_recording(self.app_handle.clone(), settings.input_device.clone());
        if let Ok(info) = &stream {
            let fallback = info
                .missing_device
                .as_ref()
                .map(|m| format!(" (chosen \"{}\" not found, using default)", m))
                .unwrap_or_default();
            log_stage_event(
                &get_data_dir(),
                "REC_START",
                &format!(
                    "Recording started by {} on \"{}\" at {} Hz, {} ch{}",
                    trigger, info.device, info.sample_rate, info.channels, fallback
                ),
            );
            *self.recording_start.lock().unwrap() = Some(RecordingStart { at: Instant::now(), trigger });
        }
        if let Err(e) = stream {
            log_stage_event(&get_data_dir(), "REC_START_FAILED", &format!("Recording requested by {} failed: {}", trigger, e));
            log::error!("Failed to start recording: {}", e);
            {
                // Idle, not Error: pressing the hotkey again retries right away.
                let mut p = self.phase.lock().unwrap();
                *p = AssistantPhase::Idle;
            }
            crate::tray::set_tray_recording(&self.app_handle, false, self.is_model_loaded());
            let session = self.session_id.load(Ordering::SeqCst);
            let subtitle = format!("Couldn't open the microphone: {}. Check {}.", e.trim_end_matches('.'), MIC_SETTING);
            show_long_error(&self.app_handle, &self.session_id, session, "Recording Failed", &subtitle);
            return Err(e);
        }

        // 5. Pre-warm active model asynchronously while user is speaking (0 wait on finish!)
        let app_handle_cl = self.app_handle.clone();
        let transcriber_cl = Arc::clone(&self.transcriber);
        let parakeet_cl = Arc::clone(&self.parakeet_transcriber);
        let model_name = settings.model_name.clone();

        tauri::async_runtime::spawn(async move {
            let is_parakeet = model_name == "parakeet-tdt-0.6b-v3";
            if is_parakeet {
                // Single Model Policy: unload Whisper
                {
                    let mut tr = transcriber_cl.lock().unwrap();
                    tr.unload();
                }
                if let Ok(dir) = crate::parakeet::ParakeetTranscriber::ensure_model(&app_handle_cl).await {
                    let mut pk = parakeet_cl.lock().unwrap();
                    let _ = pk.prewarm(&dir);
                    crate::tray::update_tray_model_status(&app_handle_cl, true);
                }
            } else {
                // Single Model Policy: unload Parakeet
                {
                    let mut pk = parakeet_cl.lock().unwrap();
                    pk.unload();
                }
                if let Ok(path) = crate::transcribe::Transcriber::ensure_model(&app_handle_cl, &model_name).await {
                    let mut tr = transcriber_cl.lock().unwrap();
                    let _ = tr.prewarm(&path);
                    crate::tray::update_tray_model_status(&app_handle_cl, true);
                }
            }
        });

        Ok(())
    }

    fn stop_listening_and_process(&self, trigger: &'static str) -> Result<(), String> {
        let pipeline_start = Instant::now();

        {
            let mut phase = self.phase.lock().unwrap();
            *phase = AssistantPhase::Transcribing;
        }
        crate::tray::set_tray_recording(&self.app_handle, false, self.is_model_loaded());
        // A mic warning may have enlarged the pill; processing states use the normal size.
        set_pill_window_size(&self.app_handle, PILL_WINDOW);

        {
            let mut t = self.last_activity.lock().unwrap();
            *t = Instant::now();
        }

        let settings = self.get_settings();
        let target_norm = crate::translate::normalize_lang(&settings.target_lang);
        let transcribing_subtitle = if target_norm == "none" {
            "Voice to Text".to_string()
        } else {
            "Converting speech to text".to_string()
        };

        // Emit transcribing state immediately so UI updates instantaneously upon hotkey release
        let _ = self.app_handle.emit(
            "assistant-state-changed",
            serde_json::json!({
                "state": "transcribing",
                "title": "Transcribing…",
                "subtitle": transcribing_subtitle
            }),
        );

        // Stop earcon plays on key release; the recorder discards audio captured after this point,
        // so the chime is not transcribed.
        if settings.sound_effect {
            crate::sound::play_sound(crate::sound::AppSound::StopRecording);
        }

        let captured = self.recorder.stop_recording(crate::denoise::strength_wet(&settings.noise_reduction));
        log_recording_stop(&self.recording_start, trigger, &captured.stats);

        let app_handle = self.app_handle.clone();
        let phase_arc = Arc::clone(&self.phase);
        let transcriber_arc = Arc::clone(&self.transcriber);
        let parakeet_arc = Arc::clone(&self.parakeet_transcriber);
        let history_arc = Arc::clone(&self.history);
        let last_activity_arc = Arc::clone(&self.last_activity);
        let pipeline_logger_arc = Arc::clone(&self.pipeline_logger);
        let session_id_arc = Arc::clone(&self.session_id);
        let pending_retry_arc = Arc::clone(&self.pending_retry);
        let session = self.session_id.load(Ordering::SeqCst);

        // Run processing in background thread using Tauri's async runtime
        tauri::async_runtime::spawn(async move {
            let is_cancelled = || session_id_arc.load(Ordering::SeqCst) != session;
            let data_dir = get_data_dir();
            if is_cancelled() {
                log_stage_event(&data_dir, "CANCELLED", "Aborted transcription: cancelled by user");
                return;
            }
            // Leveled 16 kHz speech; history gets the native-rate copy from `captured`.
            let raw_samples = &captured.speech;
            let duration_sec = raw_samples.len() as f32 / 16000.0;
            log_stage_event(&data_dir, "AUDIO", &format!("Captured {} samples ({:.2} s)", raw_samples.len(), duration_sec));
            let plan = captured.level_plan();
            log_stage_event(&data_dir, "LEVEL", &format!("Voice leveling: gain {:+.1} dB, make-up {:+.1} dB", plan.gain_db, plan.makeup_db));

            // Save full original audio (uncut) to latest_recording.wav for playback in UI
            let latest_wav = data_dir.join("latest_recording.wav");
            let _ = crate::history::write_wav_file(&latest_wav, &raw_samples, 16000);

            if duration_sec < 0.2 {
                log_stage_event(&data_dir, "AUDIO", "Audio buffer too short (<0.2s), aborting transcription.");
                {
                    let mut p = phase_arc.lock().unwrap();
                    *p = AssistantPhase::Error;
                }
                let _ = app_handle.emit(
                    "assistant-state-changed",
                    serde_json::json!({
                        "state": "error",
                        "title": "Recording Failed",
                        "subtitle": "Audio too short (held < 0.2s)"
                    }),
                );
                tokio::time::sleep(Duration::from_millis(2500)).await;
                {
                    let mut p = phase_arc.lock().unwrap();
                    *p = AssistantPhase::Idle;
                }
                let _ = app_handle.emit(
                    "assistant-state-changed",
                    serde_json::json!({
                        "state": "idle",
                        "title": "Ready",
                        "subtitle": null
                    }),
                );
                hide_main_window(&app_handle);
                return;
            }

            // Step 1: Silero VAD silence trimming with 400ms padding
            let vad_res = trim_silence(&raw_samples, 16000);
            log_stage_event(&data_dir, "VAD", &format!("VAD completed in {} ms: original {:.2} s → trimmed {:.2} s (cut {:.2} s silence)", vad_res.duration_ms, vad_res.original_duration_sec, vad_res.trimmed_duration_sec, vad_res.silence_removed_sec));

            // Save trimmed (VAD) audio to latest_vad_trimmed.wav for debugging
            let latest_vad_wav = data_dir.join("latest_vad_trimmed.wav");
            let _ = crate::history::write_wav_file(&latest_vad_wav, &vad_res.samples, 16000);

            if is_cancelled() {
                log_stage_event(&data_dir, "CANCELLED", "Aborted before model inference: cancelled by user");
                return;
            }

            // Saved before inference so a failed transcription still keeps the whole recording.
            let session_audio = session_audio_filename(&data_dir, &settings.storage_mode, &captured);

            let is_parakeet = settings.model_name == "parakeet-tdt-0.6b-v3";

            // Step 2 & 3: Model inference
            let (raw_text, detected_lang, whisper_inference_ms, thread_count, whisper_raw_output, segments_count) = if is_parakeet {
                // Single Model Policy: drop Whisper from RAM
                {
                    let mut tr = transcriber_arc.lock().unwrap();
                    tr.unload();
                }
                log_stage_event(&data_dir, "PARAKEET_START", &format!("Starting Parakeet TDT inference on {} samples ({:.2} s)...", vad_res.samples.len(), vad_res.trimmed_duration_sec));
                let model_dir = match ensure_model_for_recording(&app_handle, &settings.model_name, &transcribing_subtitle).await {
                    Ok(path) => {
                        log_stage_event(&data_dir, "MODEL", &format!("Using Parakeet model at {:?}", path));
                        path
                    },
                    Err(e) => {
                        let vitals = crate::vitals::SystemVitals::collect();
                        log_stage_event(&data_dir, "MODEL_ERROR", &format!("Failed to ensure Parakeet model: {}\n{}", e, vitals.format_report()));
                        keep_failed_recording(&history_arc, &pipeline_logger_arc, &app_handle, &captured, &settings, &vad_res, duration_sec, &data_dir, &session_audio, pipeline_start.elapsed().as_millis() as u64, &format!("Model download failed: {}", e));
                        log::error!("Failed to ensure Parakeet model: {}", e);
                        {
                            let mut p = phase_arc.lock().unwrap();
                            *p = AssistantPhase::Error;
                        }
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "error",
                                "title": "Model Download Failed",
                                "subtitle": format!("{}. {}", e, vitals.short_summary())
                            }),
                        );
                        tokio::time::sleep(Duration::from_millis(3500)).await;
                        let mut p = phase_arc.lock().unwrap();
                        *p = AssistantPhase::Idle;
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "idle",
                                "title": "Ready",
                                "subtitle": null
                            }),
                        );
                        hide_main_window(&app_handle);
                        return;
                    }
                };

                let parakeet_clone = Arc::clone(&parakeet_arc);
                let vad_samples = vad_res.samples.clone();
                let model_dir_clone = model_dir.clone();

                let timeout = inference_timeout(vad_res.trimmed_duration_sec);
                let pk_transcribe_res = tokio::time::timeout(
                    timeout,
                    tokio::task::spawn_blocking(move || {
                        let mut pk = parakeet_clone.lock().unwrap();
                        pk.transcribe(&vad_samples, &model_dir_clone)
                    }),
                )
                .await;

                match pk_transcribe_res {
                    Ok(Ok(Ok(res))) => {
                        let raw_out = if res.0.is_empty() {
                            "Parakeet TDT returned empty string (0 tokens)".to_string()
                        } else {
                            format!("Parakeet TDT Output: \"{}\"", res.0)
                        };
                        log_stage_event(&data_dir, "PARAKEET_DONE", &format!("Parakeet inference took {} ms | text: \"{}\"", res.2, res.0));
                        (res.0, res.1, res.2, res.3, raw_out, 1)
                    },
                    Ok(Ok(Err(e))) => {
                        let vitals = crate::vitals::SystemVitals::collect();
                        log_stage_event(&data_dir, "PARAKEET_ERROR", &format!("Parakeet transcription error: {}\n{}", e, vitals.format_report()));
                        keep_failed_recording(&history_arc, &pipeline_logger_arc, &app_handle, &captured, &settings, &vad_res, duration_sec, &data_dir, &session_audio, pipeline_start.elapsed().as_millis() as u64, &format!("Transcription failed: {}", e));
                        log::error!("Parakeet transcription error: {}", e);
                        {
                            let mut p = phase_arc.lock().unwrap();
                            *p = AssistantPhase::Error;
                        }
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "error",
                                "title": "Transcription Failed",
                                "subtitle": format!("{}. {}", e, vitals.short_summary())
                            }),
                        );
                        tokio::time::sleep(Duration::from_millis(3500)).await;
                        let mut p = phase_arc.lock().unwrap();
                        *p = AssistantPhase::Idle;
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "idle",
                                "title": "Ready",
                                "subtitle": null
                            }),
                        );
                        hide_main_window(&app_handle);
                        return;
                    },
                    Ok(Err(join_err)) => {
                        log::error!("Parakeet worker panicked: {:?}", join_err);
                        (String::new(), "auto".to_string(), 0, 4, format!("Parakeet Panic: {:?}", join_err), 0)
                    },
                    Err(_timeout) => {
                        let vitals = crate::vitals::SystemVitals::collect();
                        let vitals_report = vitals.format_report();
                        log_stage_event(&data_dir, "TRANSCRIPTION_TIMEOUT", &format!("Parakeet timed out after {}s!\n{}", timeout.as_secs(), vitals_report));
                        keep_failed_recording(&history_arc, &pipeline_logger_arc, &app_handle, &captured, &settings, &vad_res, duration_sec, &data_dir, &session_audio, pipeline_start.elapsed().as_millis() as u64, &format!("Transcription timed out after {}s", timeout.as_secs()));
                        log::error!("Parakeet timed out after {}s!\n{}", timeout.as_secs(), vitals_report);

                        let vitals_log = data_dir.join("system_vitals_on_timeout.log");
                        let _ = std::fs::write(&vitals_log, format!("{}\nTimestamp: {}\nAudio Duration: {:.2}s\nModel: Parakeet TDT\n", vitals_report, current_timestamp(), vad_res.trimmed_duration_sec));

                        {
                            let mut p = phase_arc.lock().unwrap();
                            *p = AssistantPhase::Error;
                        }
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "error",
                                "title": "Transcription Timed Out",
                                "subtitle": format!("Parakeet {}s timeout. {}", timeout.as_secs(), vitals.short_summary())
                            }),
                        );
                        tokio::time::sleep(Duration::from_millis(3500)).await;
                        let mut p = phase_arc.lock().unwrap();
                        *p = AssistantPhase::Idle;
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "idle",
                                "title": "Ready",
                                "subtitle": null
                            }),
                        );
                        hide_main_window(&app_handle);
                        return;
                    }
                }
            } else {
                // Single Model Policy: drop Parakeet from RAM
                {
                    let mut pk = parakeet_arc.lock().unwrap();
                    pk.unload();
                }
                let model_path = match ensure_model_for_recording(&app_handle, &settings.model_name, &transcribing_subtitle).await {
                    Ok(path) => {
                        log_stage_event(&data_dir, "MODEL", &format!("Using Whisper model {} at {:?}", settings.model_name, path));
                        path
                    },
                    Err(e) => {
                        let vitals = crate::vitals::SystemVitals::collect();
                        log_stage_event(&data_dir, "MODEL_ERROR", &format!("Failed to ensure model {}: {}\n{}", settings.model_name, e, vitals.format_report()));
                        keep_failed_recording(&history_arc, &pipeline_logger_arc, &app_handle, &captured, &settings, &vad_res, duration_sec, &data_dir, &session_audio, pipeline_start.elapsed().as_millis() as u64, &format!("Model download failed: {}", e));
                        log::error!("Failed to ensure Whisper model: {}", e);
                        {
                            let mut p = phase_arc.lock().unwrap();
                            *p = AssistantPhase::Error;
                        }
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "error",
                                "title": "Model Download Failed",
                                "subtitle": format!("{}. {}", e, vitals.short_summary())
                            }),
                        );
                        tokio::time::sleep(Duration::from_millis(3500)).await;
                        let mut p = phase_arc.lock().unwrap();
                        *p = AssistantPhase::Idle;
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "idle",
                                "title": "Ready",
                                "subtitle": null
                            }),
                        );
                        hide_main_window(&app_handle);
                        return;
                    }
                };

                log_stage_event(&data_dir, "WHISPER_START", &format!("Starting transcription on {} samples ({:.2} s)...", vad_res.samples.len(), vad_res.trimmed_duration_sec));
                let samples_clone = vad_res.samples.clone();
                let model_path_clone = model_path.clone();
                let transcriber_clone = Arc::clone(&transcriber_arc);

                let lang_hint = crate::transcribe::map_language_hint(&settings.source_lang);
                let timeout = inference_timeout(vad_res.trimmed_duration_sec);
                let inference_res = tokio::time::timeout(
                    timeout,
                    tokio::task::spawn_blocking(move || {
                        let mut tr = transcriber_clone.lock().unwrap();
                        tr.transcribe_detailed(&samples_clone, &model_path_clone, false, lang_hint)
                    }),
                )
                .await;

                match inference_res {
                    Ok(Ok(Ok(mut res))) => {
                        log_stage_event(&data_dir, "WHISPER_DONE", &format!("Inference took {} ms | detected_lang: \"{}\" | text: \"{}\"", res.inference_ms, res.detected_lang, res.text));

                        // Automatic re-recognition check for excluded languages:
                        if !settings.excluded_languages.trim().is_empty()
                            && crate::translate::is_language_excluded(&res.detected_lang, &res.text, &settings.excluded_languages)
                        {
                            let fallback_lang = crate::translate::get_fallback_language_for_excluded(
                                &settings.source_lang,
                                &settings.target_lang,
                                &res.text,
                                &settings.excluded_languages,
                            );

                            log_stage_event(
                                &data_dir,
                                "EXCLUDED_LANG_DETECTED",
                                &format!(
                                    "Detected excluded language '{}' (text: \"{}\"). Automatically re-transcribing with forced '{}'...",
                                    res.detected_lang, res.text, fallback_lang
                                ),
                            );

                            let samples_retry = vad_res.samples.clone();
                            let model_retry = model_path.clone();
                            let transcriber_retry = Arc::clone(&transcriber_arc);
                            let forced_hint = crate::transcribe::map_language_hint(&fallback_lang);

                            let retry_res = tokio::time::timeout(
                                timeout,
                                tokio::task::spawn_blocking(move || {
                                    let mut tr = transcriber_retry.lock().unwrap();
                                    tr.transcribe_detailed(&samples_retry, &model_retry, false, forced_hint)
                                }),
                            )
                            .await;

                            if let Ok(Ok(Ok(recovered))) = retry_res {
                                log_stage_event(
                                    &data_dir,
                                    "EXCLUDED_LANG_RECOVERED",
                                    &format!(
                                        "Successfully re-transcribed to forced '{}' in {} ms: \"{}\"",
                                        fallback_lang, recovered.inference_ms, recovered.text
                                    ),
                                );
                                res = recovered;
                            }
                        }

                        (res.text, res.detected_lang, res.inference_ms, res.thread_count, res.raw_output, res.segments.len())
                    },
                    Ok(Ok(Err(e))) => {
                        let vitals = crate::vitals::SystemVitals::collect();
                        log_stage_event(&data_dir, "WHISPER_ERROR", &format!("Whisper transcription error: {}\n{}", e, vitals.format_report()));
                        keep_failed_recording(&history_arc, &pipeline_logger_arc, &app_handle, &captured, &settings, &vad_res, duration_sec, &data_dir, &session_audio, pipeline_start.elapsed().as_millis() as u64, &format!("Transcription failed: {}", e));
                        log::error!("Transcription error: {}", e);
                        {
                            let mut p = phase_arc.lock().unwrap();
                            *p = AssistantPhase::Error;
                        }
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "error",
                                "title": "Transcription Failed",
                                "subtitle": format!("{}. {}", e, vitals.short_summary())
                            }),
                        );
                        tokio::time::sleep(Duration::from_millis(3500)).await;
                        let mut p = phase_arc.lock().unwrap();
                        *p = AssistantPhase::Idle;
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "idle",
                                "title": "Ready",
                                "subtitle": null
                            }),
                        );
                        hide_main_window(&app_handle);
                        return;
                    },
                    Ok(Err(join_err)) => {
                        log::error!("Whisper worker panicked: {:?}", join_err);
                        (String::new(), "unknown".to_string(), 0, 4, format!("Whisper Panic: {:?}", join_err), 0)
                    },
                    Err(_timeout) => {
                        let vitals = crate::vitals::SystemVitals::collect();
                        let vitals_report = vitals.format_report();
                        log_stage_event(&data_dir, "TRANSCRIPTION_TIMEOUT", &format!("Transcription timed out after {}s!\n{}", timeout.as_secs(), vitals_report));
                        keep_failed_recording(&history_arc, &pipeline_logger_arc, &app_handle, &captured, &settings, &vad_res, duration_sec, &data_dir, &session_audio, pipeline_start.elapsed().as_millis() as u64, &format!("Transcription timed out after {}s", timeout.as_secs()));
                        log::error!("Transcription timed out after {}s!\n{}", timeout.as_secs(), vitals_report);

                        let vitals_log_path = data_dir.join("system_vitals_on_timeout.log");
                        let _ = std::fs::write(
                            &vitals_log_path,
                            format!("{}\nTimestamp: {}\nAudio Duration: {:.2}s\nModel: {}\n",
                                vitals_report, current_timestamp(), vad_res.trimmed_duration_sec, settings.model_name)
                        );

                        {
                            let mut p = phase_arc.lock().unwrap();
                            *p = AssistantPhase::Error;
                        }

                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "error",
                                "title": "Transcription Timed Out",
                                "subtitle": format!("{}s timeout on {}. {}", timeout.as_secs(), settings.model_name, vitals.short_summary())
                            }),
                        );

                        tokio::time::sleep(Duration::from_millis(3500)).await;
                        let mut p = phase_arc.lock().unwrap();
                        *p = AssistantPhase::Idle;
                        let _ = app_handle.emit(
                            "assistant-state-changed",
                            serde_json::json!({
                                "state": "idle",
                                "title": "Ready",
                                "subtitle": null
                            }),
                        );
                        hide_main_window(&app_handle);
                        return;
                    }
                }
            };

            crate::tray::update_tray_model_status(&app_handle, true);

            if raw_text.is_empty() || raw_text == "[BLANK_AUDIO]" {
                let (chunks, mut actions) = chunk_diagnostics(
                    settings.audio_chunking,
                    duration_sec,
                    vad_res.trimmed_duration_sec,
                    vad_res.silence_removed_sec,
                    raw_samples.len(),
                );
                actions.push("[RESULT] Audio was blank or below confidence threshold".to_string());

                let empty_log = TranscriptionDiagnosticLog {
                    id: pipeline_logger_arc.next_id(),
                    timestamp: current_timestamp(),
                    audio_duration_sec: duration_sec,
                    audio_samples_count: raw_samples.len(),
                    vad_trim_ms: vad_res.duration_ms,
                    vad_original_sec: vad_res.original_duration_sec,
                    vad_trimmed_sec: vad_res.trimmed_duration_sec,
                    vad_silence_removed_sec: vad_res.silence_removed_sec,
                    model_name: settings.model_name.clone(),
                    gpu_metal_active: Transcriber::is_metal_supported(),
                    threads_count: thread_count,
                    whisper_inference_ms,
                    whisper_speed_factor: 0.0,
                    detected_lang: detected_lang.clone(),
                    raw_text: if raw_text.is_empty() { "[No speech recognized]".to_string() } else { raw_text.clone() },
                    translation_skipped: true,
                    translation_skip_reason: "Empty transcription / Blank audio".to_string(),
                    translation_ms: 0,
                    translation_error: None,
                    final_text: String::new(),
                    clipboard_paste_ms: 0,
                    history_save_ms: 0,
                    total_pipeline_ms: pipeline_start.elapsed().as_millis() as u64,
                    audio_filename: session_audio.clone(),
                    vad_audio_filename: Some("latest_vad_trimmed.wav".to_string()),
                    whisper_raw_output: Some(whisper_raw_output.clone()),
                    segments_count,
                    chunk_events: chunks,
                    whole_track: !settings.audio_chunking,
                    action_logs: actions,
                };
                pipeline_logger_arc.add_log(empty_log.clone(), &data_dir);
                let _ = app_handle.emit("transcription-diagnostic-log", &empty_log);

                {
                    let mut p = phase_arc.lock().unwrap();
                    *p = AssistantPhase::Error;
                }
                let (title, subtitle) = if is_digital_silence(&raw_samples) {
                    // The OS delivers zeros instead of an error when microphone access is blocked.
                    log_stage_event(&data_dir, "AUDIO", "Recording is digital silence: microphone blocked or muted");
                    ("Microphone Is Silent", "Allow mic access in privacy settings")
                } else if raw_text == "[BLANK_AUDIO]" {
                    ("Transcription Failed", "Blank audio — No voice detected")
                } else {
                    ("Transcription Failed", "No speech recognized in recording")
                };
                let _ = app_handle.emit(
                    "assistant-state-changed",
                    serde_json::json!({
                        "state": "error",
                        "title": title,
                        "subtitle": subtitle
                    }),
                );
                tokio::time::sleep(Duration::from_millis(2800)).await;
                {
                    let mut p = phase_arc.lock().unwrap();
                    *p = AssistantPhase::Idle;
                }
                let _ = app_handle.emit(
                    "assistant-state-changed",
                    serde_json::json!({
                        "state": "idle",
                        "title": "Ready",
                        "subtitle": null
                    }),
                );
                hide_main_window(&app_handle);
                return;
            }

            if is_cancelled() {
                log_stage_event(&data_dir, "CANCELLED", "Aborted before language detection / translation: cancelled by user");
                return;
            }

            // Step 3.5: LLM-friendly post-processing (wraps text_normalizer)
            let post_config = crate::post_processor::PostProcessorConfig {
                normalization: crate::text_normalizer::NormalizationOptions {
                    enabled: settings.text_normalization,
                    remove_fillers: settings.remove_filler_words,
                    convert_numbers: settings.convert_numbers,
                    remove_stutters: settings.remove_stutters,
                },
                apply_self_corrections: settings.apply_self_corrections,
                remove_noise_markers: settings.remove_noise_markers,
                collapse_redundancy: settings.collapse_redundancy,
                annotate_ambiguity: settings.annotate_ambiguity,
                normalize_structured_values: settings.normalize_structured_values,
            };
            let post_result = crate::post_processor::post_process(&raw_text, post_config);
            let normalized_source_text = if !post_result.clean_text.is_empty() {
                post_result.clean_text.clone()
            } else {
                raw_text.clone()
            };

            if normalized_source_text != raw_text {
                log_stage_event(
                    &data_dir,
                    "POST_PROCESSOR",
                    &format!(
                        "Post-processed transcript:\n  Raw: \"{}\"\n  Clean: \"{}\"{}",
                        raw_text,
                        normalized_source_text,
                        if post_result.requires_clarification {
                            format!("\n  ⚠ Uncertain spans: {:?}", post_result.uncertain_spans)
                        } else {
                            String::new()
                        }
                    ),
                );
            }

            // Step 3.6: Confidential text masking and redaction
            let mask_config = mask_config_from(&settings);

            let pre_translate_text = if mask_config.enabled {
                let masked = crate::masker::apply_masking(&normalized_source_text, &mask_config);
                if masked != normalized_source_text {
                    log_stage_event(
                        &data_dir,
                        "CONFIDENTIAL_MASK",
                        &format!(
                            "Masked confidential tokens before translation:\n  From: \"{}\"\n  To: \"{}\"",
                            normalized_source_text, masked
                        ),
                    );
                }
                masked
            } else {
                normalized_source_text.clone()
            };

            // Step 4: Language detection and instant skip check (0.0001s, 0 network calls)
            // Reconcile Whisper's own LID (98 langs incl. uk) with text-based heuristic:
            // - Whisper is authoritative when it returns a non-auto language
            // - Fall back to detect_text_language only when Whisper says auto/unknown
            let whisper_detected = normalize_lang(&detected_lang);
            let text_detected = crate::translate::detect_text_language(&normalized_source_text);
            let detected_effective = if whisper_detected == "unknown" || whisper_detected == "auto" {
                text_detected.clone()
            } else if (whisper_detected == "russian" || whisper_detected == "ukrainian")
                && text_detected != "unknown"
                && text_detected != whisper_detected
            {
                // Ukrainian is the most common confusion: preserve text signal on uk/ru disagreement
                // Prefer the text heuristic when cyrillic script + glyph check contradicts Whisper.
                if text_detected == "ukrainian" || text_detected == "russian" {
                    crate::translate::detect_text_language(&normalized_source_text)
                } else {
                    whisper_detected.clone()
                }
            } else {
                whisper_detected.clone()
            };
            // Keep detected_lang as the canonical value for logs/history (what Whisper reported)
            // but use detected_effective for skip/translate decisions
            log_stage_event(&data_dir, "LANG_DETECT", &format!("Whisper='{}' text='{}' → effective='{}'", detected_lang, text_detected, detected_effective));

            let target_norm = crate::translate::normalize_lang(&settings.target_lang);
            let wants_english = target_norm == "english";
            let is_english = crate::translate::normalize_lang(&detected_effective) == "english";

            let provider_norm = settings.translation_provider.to_lowercase();
            let is_no_trans = provider_norm.contains("no")
                || provider_norm.contains("none")
                || target_norm == "none";
            let is_custom = provider_norm.contains("custom");
            let is_llm = !is_no_trans && !is_custom;

            let route = TranslationRoute::from_settings(&settings);

            let can_translate = if is_no_trans {
                false
            } else if is_custom {
                !settings.custom_api_url.trim().is_empty()
            } else if is_llm {
                if route.endpoint.contains("localhost") || route.endpoint.contains("127.0.0.1") {
                    true
                } else {
                    !route.key.is_empty()
                }
            } else {
                false
            };

            let skip_check = should_skip(
                &normalized_source_text,
                &detected_effective,
                &settings.source_lang,
                &settings.target_lang,
            );

            let source_norm = crate::translate::normalize_lang(&settings.source_lang);
            let (skip_translation, skip_reason) = if is_no_trans {
                (true, "No translation mode (Voice-to-Text only)".to_string())
            } else if skip_check {
                if crate::translate::normalize_lang(&detected_effective) == target_norm {
                    (true, format!("Already in target language ({})", settings.target_lang))
                } else if source_norm != "auto" && source_norm != "unknown" && crate::translate::normalize_lang(&detected_effective) != source_norm {
                    (true, format!("Spoken '{}' does not match configured source language '{}'", detected_effective, settings.source_lang))
                } else {
                    (true, "Language check skipped translation".to_string())
                }
            } else if !can_translate && !wants_english {
                (true, format!("Translation provider '{}' is not configured", settings.translation_provider))
            } else {
                (false, "Translation required".to_string())
            };

            let mut translation_failed = false;
            let mut translation_fail_reason = String::new();
            // Full description for the diagnostic log: languages, provider, model, endpoint and error.
            let mut translation_error_detail: Option<String> = None;
            // Only service translations can be retried; local Whisper translation needs the audio.
            let mut retryable = false;

            let trans_start = Instant::now();
            let (final_text, translation_ms) = if skip_translation {
                log_stage_event(&data_dir, "TRANSLATE_SKIP", &format!("Skipped translation: {}", skip_reason));
                (pre_translate_text.clone(), 0)
            } else if can_translate {
                let from_name = crate::translate::format_lang_name(&detected_effective);
                let to_name = crate::translate::format_lang_name(&settings.target_lang);
                let provider_label = route.label();

                log_stage_event(
                    &data_dir,
                    "TRANSLATE_START",
                    &format!(
                        "Translating from {} to {} with {}: \"{}\"",
                        from_name, to_name, route.describe(&settings), pre_translate_text
                    ),
                );
                // Step 5A: Translate with configured provider
                {
                    let mut p = phase_arc.lock().unwrap();
                    *p = AssistantPhase::Translating;
                }

                let _ = app_handle.emit(
                    "assistant-state-changed",
                    serde_json::json!({
                        "state": "translating",
                        "title": "Translating…",
                        "subtitle": format!("{} → {} ({})", from_name, to_name, provider_label)
                    }),
                );

                let (trans_result, ms) = match route.translate(&settings, &pre_translate_text).await {
                    Ok(t) => {
                        let ms = trans_start.elapsed().as_millis() as u64;
                        log_stage_event(&data_dir, "TRANSLATE_DONE", &format!("{} translated in {} ms: \"{}\"", provider_label, ms, t));
                        (t, ms)
                    }
                    Err(e) => {
                        let ms = trans_start.elapsed().as_millis() as u64;
                        let vitals = crate::vitals::SystemVitals::collect();
                        let detail = format!(
                            "{} → {} via {} failed after {} ms: {}",
                            from_name, to_name, route.describe(&settings), ms, e
                        );
                        log_stage_event(&data_dir, "TRANSLATE_ERROR", &format!("{}\n{}", detail, vitals.format_report()));
                        log::error!("Translation failed: {}. Pasting the spoken text instead.", detail);
                        translation_failed = true;
                        translation_fail_reason = e;
                        translation_error_detail = Some(detail);
                        retryable = true;
                        (pre_translate_text.clone(), 0)
                    }
                };
                (trans_result, ms)
            } else if wants_english && !is_english {
                log_stage_event(&data_dir, "TRANSLATE_WHISPER_START", &format!("Translating locally to English in RAM: \"{}\"", raw_text));
                // Step 5B: Local Whisper translation to English in RAM (0 API key needed)
                {
                    let mut p = phase_arc.lock().unwrap();
                    *p = AssistantPhase::Translating;
                }

                let _ = app_handle.emit(
                    "assistant-state-changed",
                    serde_json::json!({
                        "state": "translating",
                        "title": "Translating locally…",
                        "subtitle": format!("{} → English (Whisper AI)", detected_lang)
                    }),
                );

                let whisper_model_path = Transcriber::get_model_path(
                    if is_parakeet { "ggml-base.bin" } else { &settings.model_name }
                );

                let (translated, _, _, _) = {
                    let mut tr = transcriber_arc.lock().unwrap();
                    match tr.transcribe(&vad_res.samples, &whisper_model_path, true) {
                        Ok(res) => res,
                        Err(e) => {
                            let detail = format!("{} → English via local Whisper ({:?}) failed: {}", detected_lang, whisper_model_path, e);
                            log_stage_event(&data_dir, "TRANSLATE_WHISPER_ERROR", &detail);
                            log::error!("Local translation failed: {}. Pasting the spoken text instead.", detail);
                            translation_failed = true;
                            translation_fail_reason = format!("local Whisper translation failed: {}", e);
                            translation_error_detail = Some(detail);
                            (pre_translate_text.clone(), detected_lang.clone(), 0, 4)
                        }
                    }
                };

                let ms = trans_start.elapsed().as_millis() as u64;
                log_stage_event(&data_dir, "TRANSLATE_WHISPER_DONE", &format!("Local translation finished in {} ms: \"{}\"", ms, translated));
                (translated, ms)
            } else {
                (pre_translate_text.clone(), 0)
            };

            // Step 5.5: Final confidential text masking on outgoing text
            let final_text = if mask_config.enabled {
                crate::masker::apply_masking(&final_text, &mask_config)
            } else {
                final_text
            };

            if is_cancelled() {
                log_stage_event(&data_dir, "CANCELLED", "Aborted before paste/done: cancelled by user");
                return;
            }

            // Step 6: Set phase. A failed translation goes straight back to Idle so the hotkey works while
            // the error (with its Retry button) is still on screen.
            {
                let mut p = phase_arc.lock().unwrap();
                if translation_failed {
                    *p = AssistantPhase::Idle;
                } else {
                    *p = AssistantPhase::Done;
                }
            }

            // Step 7: Copy to clipboard and auto-paste if enabled
            let paste_start = Instant::now();
            if settings.auto_paste {
                let _ = copy_and_paste(&final_text);
            } else {
                let _ = crate::paste::copy_to_clipboard(&final_text);
            }
            let paste_ms = paste_start.elapsed().as_millis() as u64;
            log_stage_event(&data_dir, "PASTE", &format!("Auto-paste {} in {} ms", if settings.auto_paste { "dispatched" } else { "skipped" }, paste_ms));

            // A failed translation is announced after it's saved to history (the retry needs its id).
            if !translation_failed {
                let _ = app_handle.emit(
                    "assistant-state-changed",
                    serde_json::json!({
                        "state": "done",
                        "title": "Copied to clipboard",
                        "subtitle": null,
                        "text": final_text
                    }),
                );
            }

            // Step 8: Save to history (save full original recorded audio, uncut!)
            let db_start = Instant::now();
            let history_source_text = if mask_config.enabled {
                crate::masker::apply_masking(&raw_text, &mask_config)
            } else {
                raw_text.clone()
            };
            let history_id = history_arc
                .add_entry(
                    &history_source_text,
                    &final_text,
                    &detected_lang,
                    &settings.target_lang,
                    duration_sec,
                    Some(&captured),
                    &settings.storage_mode,
                    settings.storage_cap_mb,
                )
                .unwrap_or(0);
            let history_ms = db_start.elapsed().as_millis() as u64;
            log_stage_event(&data_dir, "DB", &format!("Saved to database in {} ms", history_ms));

            // Remember a failed service translation so it can be retried from the pill or the tray.
            let pending = (translation_failed && retryable).then(|| PendingRetry {
                text: pre_translate_text.clone(),
                from_lang: detected_effective.clone(),
                history_id,
            });
            let can_retry = pending.is_some();
            *pending_retry_arc.lock().unwrap() = pending;
            crate::tray::set_retry_item(can_retry);

            if translation_failed {
                let what_was_pasted = if settings.auto_paste { "pasted" } else { "copied" };
                let subtitle = if can_retry {
                    format!("{}. Your spoken text was {} instead. Retry, or check your translation settings.", capitalize(&translation_fail_reason), what_was_pasted)
                } else {
                    format!("{}. Your spoken text was {} instead.", capitalize(&translation_fail_reason), what_was_pasted)
                };
                show_long_error_payload(
                    &app_handle,
                    &session_id_arc,
                    session,
                    serde_json::json!({
                        "state": "error",
                        "title": "Translation Failed",
                        "subtitle": subtitle,
                        "long": true,
                        "retry": can_retry
                    }),
                );
            }

            let total_pipeline_ms = pipeline_start.elapsed().as_millis() as u64;
            let speed_factor = if whisper_inference_ms > 0 {
                (vad_res.trimmed_duration_sec * 1000.0) / (whisper_inference_ms as f32)
            } else {
                0.0
            };
            log_stage_event(&data_dir, "COMPLETE", &format!("Pipeline complete in {} ms! Speed factor: {:.1}x", total_pipeline_ms, speed_factor));

            let (chunks, mut actions) = chunk_diagnostics(
                    settings.audio_chunking,
                duration_sec,
                vad_res.trimmed_duration_sec,
                vad_res.silence_removed_sec,
                raw_samples.len(),
            );
            actions.push(format!("[MODEL] Transcribed with {} in {} ms ({:.1}x real-time)", settings.model_name, whisper_inference_ms, speed_factor));
            if skip_translation {
                actions.push(format!("[TRANSLATION] Skipped ({})", skip_reason));
            } else if let Some(detail) = &translation_error_detail {
                actions.push(format!("[TRANSLATION] FAILED: {}", detail));
                actions.push("[TRANSLATION] Pasted the spoken text instead".to_string());
                if can_retry {
                    actions.push("[TRANSLATION] Retry available from the pill or the tray menu".to_string());
                }
            } else if can_translate {
                actions.push(format!("[TRANSLATION] Translated in {} ms via {}", translation_ms, route.describe(&settings)));
            } else {
                actions.push(format!("[TRANSLATION] Translated locally in {} ms via Whisper", translation_ms));
            }
            actions.push(format!("[DISPATCH] Pasted output in {} ms", paste_ms));
            actions.push(format!("[COMPLETE] Total pipeline finished in {} ms", total_pipeline_ms));

            // Step 9: Diagnostic Logging (stdout report, file write, in-memory ring buffer, Tauri event)
            let log_entry = TranscriptionDiagnosticLog {
                id: pipeline_logger_arc.next_id(),
                timestamp: current_timestamp(),
                audio_duration_sec: duration_sec,
                audio_samples_count: raw_samples.len(),
                vad_trim_ms: vad_res.duration_ms,
                vad_original_sec: vad_res.original_duration_sec,
                vad_trimmed_sec: vad_res.trimmed_duration_sec,
                vad_silence_removed_sec: vad_res.silence_removed_sec,
                model_name: settings.model_name.clone(),
                gpu_metal_active: Transcriber::is_metal_supported(),
                threads_count: thread_count,
                whisper_inference_ms,
                whisper_speed_factor: speed_factor,
                detected_lang,
                raw_text: raw_text.clone(),
                translation_skipped: skip_translation,
                translation_skip_reason: skip_reason,
                translation_ms,
                translation_error: translation_error_detail.clone(),
                final_text: final_text.clone(),
                clipboard_paste_ms: paste_ms,
                history_save_ms: history_ms,
                total_pipeline_ms,
                audio_filename: session_audio.clone(),
                vad_audio_filename: Some("latest_vad_trimmed.wav".to_string()),
                whisper_raw_output: Some(whisper_raw_output),
                segments_count,
                chunk_events: chunks,
                whole_track: !settings.audio_chunking,
                action_logs: actions,
            };

            pipeline_logger_arc.add_log(log_entry.clone(), &get_data_dir());
            let _ = app_handle.emit("transcription-diagnostic-log", &log_entry);

            // A failed translation hides itself after LONG_ERROR_HOLD (see show_long_error_payload).
            if translation_failed {
                return;
            }
            // Show the done pill briefly, then revert to Idle and hide the window.
            tokio::time::sleep(Duration::from_millis(600)).await;
            {
                let mut p = phase_arc.lock().unwrap();
                if *p == AssistantPhase::Done || *p == AssistantPhase::Error {
                    *p = AssistantPhase::Idle;
                    *last_activity_arc.lock().unwrap() = Instant::now();
                    let _ = app_handle.emit(
                        "assistant-state-changed",
                        serde_json::json!({
                            "state": "idle",
                            "title": "Ready",
                            "subtitle": null
                        }),
                    );
                    hide_main_window(&app_handle);
                }
            }
        });

        Ok(())
    }

    pub fn set_audio_device(&self, kind: &str, name: &str) -> Result<(), String> {
        let mut settings = self.get_settings();
        let opt_name = if name == "Default" {
            None
        } else {
            Some(name.to_string())
        };
        if kind == "input" {
            settings.input_device = opt_name;
        } else if kind == "output" {
            settings.output_device = opt_name;
        }
        self.save_settings(settings)
    }

    pub fn set_target_lang(&self, lang: &str) -> Result<(), String> {
        let mut settings = self.get_settings();
        settings.target_lang = lang.to_string();
        self.save_settings(settings)
    }

    pub fn set_translation_provider(&self, provider: &str) -> Result<(), String> {
        let mut settings = self.get_settings();
        settings.translation_provider = provider.to_string();
        self.save_settings(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::{capitalize, is_digital_silence, mic_problem_message, TranslationRoute};
    use crate::settings::AppSettings;

    #[test]
    fn translation_route_prefers_explicit_llm_settings() {
        let mut s = AppSettings::default();
        s.translation_provider = "LLM".into();
        s.llm_endpoint = " http://127.0.0.1:11434/v1/chat/completions ".into();
        s.llm_api_key = " key-1 ".into();
        s.llm_model = "llama3.2".into();
        let route = TranslationRoute::from_settings(&s);
        assert_eq!(route.endpoint, "http://127.0.0.1:11434/v1/chat/completions");
        assert_eq!(route.key, "key-1");
        assert_eq!(route.model, "llama3.2");
        assert_eq!(route.describe(&s), "LLM llama3.2 at http://127.0.0.1:11434/v1/chat/completions");
    }

    #[test]
    fn translation_route_custom_api_defaults_its_model() {
        let mut s = AppSettings::default();
        s.translation_provider = "Custom API".into();
        s.custom_api_url = "https://api.example.com/v1/chat/completions".into();
        s.custom_api_model = String::new();
        let route = TranslationRoute::from_settings(&s);
        assert_eq!(route.label(), "Custom API");
        assert_eq!(route.model, "gpt-4o-mini");
        assert_eq!(route.describe(&s), "Custom API gpt-4o-mini at https://api.example.com/v1/chat/completions");
    }

    #[tokio::test]
    async fn translation_route_reports_an_unreachable_service_as_an_error() {
        let mut s = AppSettings::default();
        s.translation_provider = "LLM".into();
        // Port 9 (discard) is closed on test machines, so the request fails right away.
        s.llm_endpoint = "http://127.0.0.1:9/v1/chat/completions".into();
        s.llm_model = "test-model".into();
        let err = TranslationRoute::from_settings(&s).translate(&s, "Привіт").await.unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn capitalize_first_letter_only() {
        assert_eq!(capitalize("timed out after 20 s"), "Timed out after 20 s");
        assert_eq!(capitalize(""), "");
    }
    use crate::audio::{MicProblem, MicReport};

    fn report(problem: MicProblem, missing: Option<&str>) -> MicReport {
        MicReport { problem, device: "MacBook Pro Microphone".into(), missing_device: missing.map(Into::into), reconnecting: false }
    }

    #[test]
    fn mic_messages_name_the_device_and_the_setting() {
        let (title, text) = mic_problem_message(&report(MicProblem::NoAudio, None));
        assert_eq!(title, "Microphone Not Working");
        assert!(text.contains("\"MacBook Pro Microphone\""), "{text}");
        assert!(text.contains("Settings → General → Microphone input"), "{text}");

        let (title, text) = mic_problem_message(&report(MicProblem::Silent, Some("Jabra Evolve")));
        assert_eq!(title, "Microphone Not Found");
        assert!(text.contains("\"Jabra Evolve\" isn't connected"), "{text}");
        assert!(text.contains("default \"MacBook Pro Microphone\" sends no audio"), "{text}");
    }

    #[test]
    fn digital_silence_only_for_blocked_input() {
        assert!(is_digital_silence(&[0.0; 16000]));
        assert!(is_digital_silence(&[]));
        // A quiet but live microphone still has a noise floor.
        let noise: Vec<f32> = (0..16000).map(|i| if i % 2 == 0 { 0.002 } else { -0.002 }).collect();
        assert!(!is_digital_silence(&noise));
    }
}
