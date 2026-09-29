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

/// Makes the voice pill float above full-screen apps on every Space without taking keyboard focus,
/// so the paste lands in the app the user was typing in.
///
/// The window must stay the NSWindow subclass tao created: swapping its class (e.g. to NSPanel) breaks
/// WebKit's KVO observers and aborts the app on macOS 27 when the view hierarchy is rebuilt.
pub(crate) fn apply_pill_window_behavior(win: &tauri::WebviewWindow, order_front: bool) {
    let _ = win.set_focusable(false);

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
        let send: extern "C" fn(*mut c_void, *const c_void) = std::mem::transmute(objc_msgSend as *const ());

        if let Ok(ns_win) = win.ns_window() {
            let ptr = ns_win as *mut c_void;

            // NSWindowCollectionBehaviorCanJoinAllSpaces (1) | IgnoresCycle (64) | FullScreenAuxiliary (256)
            let behavior: usize = (1 << 0) | (1 << 6) | (1 << 8);
            send_usize(ptr, sel_registerName(b"setCollectionBehavior:\0".as_ptr()), behavior);

            // NSScreenSaverWindowLevel (1000) floats directly above full-screen and active apps
            send_isize(ptr, sel_registerName(b"setLevel:\0".as_ptr()), 1000);

            if order_front {
                // Shows on the active Space immediately without activating RevFly
                send(ptr, sel_registerName(b"orderFrontRegardless\0".as_ptr()));
            }
        }
    }
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

    pub fn cancel(&self) {
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

    pub fn toggle_recording(&self) -> Result<(), String> {
        let cur_phase = { self.phase.lock().unwrap().clone() };

        match cur_phase {
            AssistantPhase::Idle => self.start_listening(),
            AssistantPhase::Listening => self.stop_listening_and_process(),
            _ => {
                // If currently processing or in Done state, ignore toggle or cancel
                Ok(())
            }
        }
    }

    fn start_listening(&self) -> Result<(), String> {
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

        // 3. Play earcon sound in background thread if enabled
        if settings.sound_effect {
            crate::sound::play_sound(crate::sound::AppSound::StartRecording);
        }

        // 4. Start recording on the dedicated audio thread
        if let Err(e) = self.recorder.start_recording(self.app_handle.clone(), settings.input_device.clone()) {
            log::error!("Failed to start recording: {}", e);
            {
                let mut p = self.phase.lock().unwrap();
                *p = AssistantPhase::Error;
            }
            let _ = self.app_handle.emit(
                "assistant-state-changed",
                serde_json::json!({
                    "state": "error",
                    "title": "Recording Failed",
                    "subtitle": "Microphone unavailable or blocked"
                }),
            );
            let app_h = self.app_handle.clone();
            let phase_arc = Arc::clone(&self.phase);
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(3200)).await;
                let mut p = phase_arc.lock().unwrap();
                *p = AssistantPhase::Idle;
                let _ = app_h.emit(
                    "assistant-state-changed",
                    serde_json::json!({
                        "state": "idle",
                        "title": "Ready",
                        "subtitle": null
                    }),
                );
                hide_main_window(&app_h);
            });
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

    fn stop_listening_and_process(&self) -> Result<(), String> {
        let pipeline_start = Instant::now();

        {
            let mut phase = self.phase.lock().unwrap();
            *phase = AssistantPhase::Transcribing;
        }
        crate::tray::set_tray_recording(&self.app_handle, false, self.is_model_loaded());

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

        let raw_samples = self.recorder.stop_recording();

        let app_handle = self.app_handle.clone();
        let phase_arc = Arc::clone(&self.phase);
        let transcriber_arc = Arc::clone(&self.transcriber);
        let parakeet_arc = Arc::clone(&self.parakeet_transcriber);
        let history_arc = Arc::clone(&self.history);
        let last_activity_arc = Arc::clone(&self.last_activity);
        let pipeline_logger_arc = Arc::clone(&self.pipeline_logger);
        let session_id_arc = Arc::clone(&self.session_id);
        let session = self.session_id.load(Ordering::SeqCst);

        // Run processing in background thread using Tauri's async runtime
        tauri::async_runtime::spawn(async move {
            let is_cancelled = || session_id_arc.load(Ordering::SeqCst) != session;
            let data_dir = get_data_dir();
            if is_cancelled() {
                log_stage_event(&data_dir, "CANCELLED", "Aborted transcription: cancelled by user");
                return;
            }
            let duration_sec = raw_samples.len() as f32 / 16000.0;
            log_stage_event(&data_dir, "AUDIO", &format!("Captured {} samples ({:.2} s)", raw_samples.len(), duration_sec));

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

                let pk_transcribe_res = tokio::time::timeout(
                    Duration::from_secs(30),
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
                        log_stage_event(&data_dir, "TRANSCRIPTION_TIMEOUT", &format!("Parakeet timed out after 30s!\n{}", vitals_report));
                        log::error!("Parakeet timed out after 30s!\n{}", vitals_report);

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
                                "subtitle": format!("Parakeet 30s timeout. {}", vitals.short_summary())
                            }),
                        );
                        let _ = history_arc.add_entry(
                            "[Transcription timed out]",
                            &format!("[Timed out after 30s: {}]", vitals.short_summary()),
                            "auto",
                            &settings.target_lang,
                            duration_sec,
                            Some(&raw_samples),
                            &settings.storage_mode,
                            settings.storage_cap_mb,
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
                let inference_res = tokio::time::timeout(
                    Duration::from_secs(30),
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
                                Duration::from_secs(15),
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
                        log_stage_event(&data_dir, "TRANSCRIPTION_TIMEOUT", &format!("Transcription timed out after 30s!\n{}", vitals_report));
                        log::error!("Transcription timed out after 30 seconds!\n{}", vitals_report);

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
                                "subtitle": format!("30s timeout on {}. {}", settings.model_name, vitals.short_summary())
                            }),
                        );

                        let _ = history_arc.add_entry(
                            "[Transcription timed out]",
                            &format!("[Timed out after 30s: {}]", vitals.short_summary()),
                            "unknown",
                            &settings.target_lang,
                            duration_sec,
                            Some(&raw_samples),
                            &settings.storage_mode,
                            settings.storage_cap_mb,
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
                let (chunks, mut actions) = crate::pipeline_logger::generate_chunk_diagnostics(
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
                    final_text: String::new(),
                    clipboard_paste_ms: 0,
                    history_save_ms: 0,
                    total_pipeline_ms: pipeline_start.elapsed().as_millis() as u64,
                    audio_filename: Some("latest_recording.wav".to_string()),
                    vad_audio_filename: Some("latest_vad_trimmed.wav".to_string()),
                    whisper_raw_output: Some(whisper_raw_output.clone()),
                    segments_count,
                    chunk_events: chunks,
                    action_logs: actions,
                };
                pipeline_logger_arc.add_log(empty_log.clone(), &data_dir);
                let _ = app_handle.emit("transcription-diagnostic-log", &empty_log);

                {
                    let mut p = phase_arc.lock().unwrap();
                    *p = AssistantPhase::Error;
                }
                let _ = app_handle.emit(
                    "assistant-state-changed",
                    serde_json::json!({
                        "state": "error",
                        "title": "Transcription Failed",
                        "subtitle": if raw_text == "[BLANK_AUDIO]" { "Blank audio — No voice detected" } else { "No speech recognized in recording" }
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
            let mask_rules = crate::masker::parse_rules(&settings.mask_words, &settings.mask_format);
            let mask_config = crate::masker::MaskConfig {
                enabled: settings.mask_confidential,
                rules: mask_rules,
                threshold: settings.mask_threshold,
                default_mask: settings.mask_format.clone(),
                mask_emails: settings.mask_emails,
                mask_phones: settings.mask_phones,
                mask_cards: settings.mask_cards,
            };

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

            let endpoint_to_use = if !settings.llm_endpoint.trim().is_empty() {
                settings.llm_endpoint.trim().to_string()
            } else if !settings.local_llm_url.trim().is_empty() {
                settings.local_llm_url.trim().to_string()
            } else {
                "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions".to_string()
            };

            let key_to_use = if !settings.llm_api_key.trim().is_empty() {
                settings.llm_api_key.trim().to_string()
            } else if !settings.api_key.trim().is_empty() {
                settings.api_key.trim().to_string()
            } else {
                settings.local_llm_api_key.trim().to_string()
            };

            let model_to_use = if !settings.llm_model.trim().is_empty() {
                settings.llm_model.trim().to_string()
            } else if !settings.gemini_model.trim().is_empty() {
                settings.gemini_model.trim().to_string()
            } else if !settings.local_llm_model.trim().is_empty() {
                settings.local_llm_model.trim().to_string()
            } else {
                "gemini-3.6-flash".to_string()
            };

            let can_translate = if is_no_trans {
                false
            } else if is_custom {
                !settings.custom_api_url.trim().is_empty()
            } else if is_llm {
                if endpoint_to_use.contains("localhost") || endpoint_to_use.contains("127.0.0.1") {
                    true
                } else {
                    !key_to_use.is_empty()
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

            let trans_start = Instant::now();
            let (final_text, translation_ms) = if skip_translation {
                log_stage_event(&data_dir, "TRANSLATE_SKIP", &format!("Skipped translation: {}", skip_reason));
                (pre_translate_text.clone(), 0)
            } else if can_translate {
                let from_name = crate::translate::format_lang_name(&detected_effective);
                let to_name = crate::translate::format_lang_name(&settings.target_lang);

                let provider_label = if is_custom {
                    "Custom API"
                } else {
                    "LLM"
                };

                let active_model = if is_custom {
                    if settings.custom_api_model.trim().is_empty() {
                        "gpt-4o-mini".to_string()
                    } else {
                        settings.custom_api_model.trim().to_string()
                    }
                } else {
                    model_to_use.clone()
                };

                log_stage_event(
                    &data_dir,
                    "TRANSLATE_START",
                    &format!(
                        "Translating from {} to {} with {} (model: {}): \"{}\"",
                        from_name, to_name, provider_label, active_model, pre_translate_text
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

                let trans_timeout = Duration::from_secs(20);
                let (trans_result, ms) = match tokio::time::timeout(
                    trans_timeout,
                    async {
                        if is_custom {
                            crate::translate::translate_with_openai_compatible(
                                &pre_translate_text,
                                &settings.custom_api_url,
                                &settings.custom_api_key,
                                &settings.source_lang,
                                &settings.target_lang,
                                &active_model,
                                &settings.prompt_template,
                            ).await
                        } else if endpoint_to_use.contains("generativelanguage.googleapis.com")
                            && !endpoint_to_use.contains("/openai")
                        {
                            crate::translate::translate_with_gemini(
                                &pre_translate_text,
                                &key_to_use,
                                &settings.source_lang,
                                &settings.target_lang,
                                &active_model,
                                &settings.prompt_template,
                            ).await
                        } else {
                            crate::translate::translate_with_openai_compatible(
                                &pre_translate_text,
                                &endpoint_to_use,
                                &key_to_use,
                                &settings.source_lang,
                                &settings.target_lang,
                                &active_model,
                                &settings.prompt_template,
                            ).await
                        }
                    },
                )
                .await
                {
                    Ok(Ok(t)) => {
                        let ms = trans_start.elapsed().as_millis() as u64;
                        log_stage_event(&data_dir, "TRANSLATE_DONE", &format!("{} translated in {} ms: \"{}\"", provider_label, ms, t));
                        (t, ms)
                    }
                    Ok(Err(e)) => {
                        let vitals = crate::vitals::SystemVitals::collect();
                        log_stage_event(&data_dir, "TRANSLATE_ERROR", &format!("{} translation error: {}\n{}", provider_label, e, vitals.format_report()));
                        log::warn!("Translation error: {}. Using normalized recognized text.", e);
                        translation_failed = true;
                        translation_fail_reason = format!("API Error ({})", e);
                        (pre_translate_text.clone(), 0)
                    }
                    Err(_timeout) => {
                        let vitals = crate::vitals::SystemVitals::collect();
                        log_stage_event(&data_dir, "TRANSLATE_TIMEOUT", &format!("{} translation timed out after 20s!\n{}", provider_label, vitals.format_report()));
                        log::warn!("Translation timed out after 20s. Using normalized recognized text.");
                        translation_failed = true;
                        translation_fail_reason = "Network timed out after 20s".to_string();
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
                            log::warn!("Local translation error: {}. Using raw recognized text.", e);
                            translation_failed = true;
                            translation_fail_reason = "Local translation failed".to_string();
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

            // Step 6: Set phase (Done or Error)
            {
                let mut p = phase_arc.lock().unwrap();
                if translation_failed {
                    *p = AssistantPhase::Error;
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

            if translation_failed {
                let _ = app_handle.emit(
                    "assistant-state-changed",
                    serde_json::json!({
                        "state": "error",
                        "title": "Translation Failed",
                        "subtitle": format!("{}. Spoken text copied.", translation_fail_reason),
                        "text": final_text
                    }),
                );
            } else {
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
            let _ = history_arc.add_entry(
                &history_source_text,
                &final_text,
                &detected_lang,
                &settings.target_lang,
                duration_sec,
                Some(&raw_samples),
                &settings.storage_mode,
                settings.storage_cap_mb,
            );
            let history_ms = db_start.elapsed().as_millis() as u64;
            log_stage_event(&data_dir, "DB", &format!("Saved to database in {} ms", history_ms));

            let total_pipeline_ms = pipeline_start.elapsed().as_millis() as u64;
            let speed_factor = if whisper_inference_ms > 0 {
                (vad_res.trimmed_duration_sec * 1000.0) / (whisper_inference_ms as f32)
            } else {
                0.0
            };
            log_stage_event(&data_dir, "COMPLETE", &format!("Pipeline complete in {} ms! Speed factor: {:.1}x", total_pipeline_ms, speed_factor));

            let (chunks, mut actions) = crate::pipeline_logger::generate_chunk_diagnostics(
                duration_sec,
                vad_res.trimmed_duration_sec,
                vad_res.silence_removed_sec,
                raw_samples.len(),
            );
            actions.push(format!("[MODEL] Transcribed with {} in {} ms ({:.1}x real-time)", settings.model_name, whisper_inference_ms, speed_factor));
            if skip_translation {
                actions.push(format!("[TRANSLATION] Skipped ({})", skip_reason));
            } else {
                actions.push(format!("[TRANSLATION] Translated in {} ms via Gemini API", translation_ms));
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
                final_text: final_text.clone(),
                clipboard_paste_ms: paste_ms,
                history_save_ms: history_ms,
                total_pipeline_ms,
                audio_filename: Some("latest_recording.wav".to_string()),
                vad_audio_filename: Some("latest_vad_trimmed.wav".to_string()),
                whisper_raw_output: Some(whisper_raw_output),
                segments_count,
                chunk_events: chunks,
                action_logs: actions,
            };

            pipeline_logger_arc.add_log(log_entry.clone(), &get_data_dir());
            let _ = app_handle.emit("transcription-diagnostic-log", &log_entry);

            // Wait default time (3.5s for error pill, 600ms for done pill) then revert to Idle and hide window
            let display_hold_ms = if translation_failed { 3500 } else { 600 };
            tokio::time::sleep(Duration::from_millis(display_hold_ms)).await;
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
