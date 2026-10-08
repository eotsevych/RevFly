pub mod app_controller;
pub mod audio;
pub mod cpu_features;
pub mod custom_models;
pub mod global_key_listener;
pub mod history;
pub mod paste;
pub mod perf;
pub mod pipeline_logger;
pub mod settings;
pub mod text_normalizer;
pub mod post_processor;
pub mod masker;
pub mod transcribe;
pub mod translate;
pub mod tray;
pub mod updater;
pub mod vad;
pub mod parakeet;
pub mod lab;
pub mod denoise;
pub mod fullscreen_pill;
pub mod leveler;
pub mod model_download;
pub mod sound;
pub mod vitals;

use std::sync::Arc;
use tauri::{AppHandle, Listener, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use app_controller::{AppController, RecordingMode};
use history::HistoryRecord;
use settings::AppSettings;

pub struct AppState {
    pub controller: Arc<AppController>,
    pub key_listener: Arc<global_key_listener::GlobalKeyListenerHandle>,
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.controller.get_settings()
}

#[tauri::command]
fn check_accessibility() -> bool {
    global_key_listener::is_accessibility_trusted()
}

#[tauri::command]
fn request_accessibility() {
    global_key_listener::request_accessibility_prompt();
}

#[tauri::command]
fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

#[tauri::command]
fn save_settings(
    state: State<'_, AppState>,
    app_handle: AppHandle,
    mut new_settings: AppSettings,
) -> Result<(), String> {
    new_settings.migrate();
    let old_settings = state.controller.get_settings();
    state.controller.save_settings(new_settings.clone())?;

    // Update global hotkeys if changed (the translate hotkey is only active in its mode)
    if old_settings.hotkey != new_settings.hotkey
        || old_settings.active_translate_hotkey() != new_settings.active_translate_hotkey()
    {
        apply_hotkeys(&app_handle, &state, &new_settings);
    }

    Ok(())
}

/// Points the modifier-key listener and the key-combo shortcuts at the settings' hotkeys.
pub(crate) fn apply_hotkeys(app_handle: &AppHandle, state: &AppState, settings: &AppSettings) {
    state.key_listener.update_hotkeys(&settings.hotkey, settings.active_translate_hotkey());
    register_global_shortcuts(app_handle, settings);
    prompt_accessibility_for_modifier_hotkeys(settings);
}

/// (Re)registers the key-combo hotkeys (e.g. `Control+Shift+Space`) with the global-shortcut
/// plugin. Modifier-only hotkeys (e.g. `RightOption`) don't parse as shortcuts; on macOS the event
/// tap in global_key_listener.rs handles those.
pub(crate) fn register_global_shortcuts(app_handle: &AppHandle, settings: &AppSettings) {
    let gs = app_handle.global_shortcut();
    let _ = gs.unregister_all();
    let main = settings.hotkey.parse::<Shortcut>().ok();
    if let Some(shortcut) = main {
        if let Err(e) = gs.register(shortcut) {
            log::warn!("Could not register hotkey {}: {}", settings.hotkey, e);
        }
    }
    if let Ok(shortcut) = settings.active_translate_hotkey().parse::<Shortcut>() {
        if Some(shortcut) == main {
            log::warn!("Translate hotkey is the same as the main hotkey; ignoring it");
        } else if let Err(e) = gs.register(shortcut) {
            log::warn!("Could not register translate hotkey {}: {}", settings.active_translate_hotkey(), e);
        }
    }
}

/// Which recording a pressed key-combo shortcut starts: the translate one if it matches
/// the active translate hotkey, otherwise the main one.
fn shortcut_mode(shortcut: &Shortcut, settings: &AppSettings) -> RecordingMode {
    let is_translate = settings.active_translate_hotkey().parse::<Shortcut>().map(|t| &t == shortcut).unwrap_or(false);
    let is_main = settings.hotkey.parse::<Shortcut>().map(|m| &m == shortcut).unwrap_or(false);
    if is_translate && !is_main {
        RecordingMode::Translate
    } else {
        RecordingMode::Main
    }
}

fn prompt_accessibility_for_modifier_hotkeys(settings: &AppSettings) {
    let uses_modifier = [settings.hotkey.as_str(), settings.active_translate_hotkey()]
        .iter()
        .any(|h| global_key_listener::hotkey_str_to_modifier_keycode(h).is_some());
    if uses_modifier && !global_key_listener::is_accessibility_trusted() {
        global_key_listener::request_accessibility_prompt();
    }
}

#[tauri::command]
fn toggle_recording(state: State<'_, AppState>) -> Result<(), String> {
    state.controller.toggle_recording("app")
}

#[tauri::command]
fn cancel_recording(state: State<'_, AppState>) -> Result<(), String> {
    state.controller.cancel("Esc in the pill");
    Ok(())
}

#[tauri::command]
fn retry_translation(state: State<'_, AppState>) -> Result<(), String> {
    state.controller.retry_translation()
}

#[tauri::command]
fn paste_original(state: State<'_, AppState>) -> Result<(), String> {
    state.controller.paste_original()
}

#[tauri::command]
fn get_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<HistoryRecord>, String> {
    state.controller.get_history(limit.unwrap_or(50))
}

#[tauri::command]
fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    state.controller.clear_history()
}

#[tauri::command]
fn get_model_status(state: State<'_, AppState>) -> serde_json::Value {
    let settings = state.controller.get_settings();
    if settings.model_name == "parakeet-tdt-0.6b-v3" {
        let exists = parakeet::ParakeetTranscriber::model_ready();
        let path = parakeet::ParakeetTranscriber::get_model_dir();
        serde_json::json!({
            "model_name": settings.model_name,
            "exists": exists,
            "path": path.to_string_lossy(),
        })
    } else {
        let exists = transcribe::Transcriber::model_exists(&settings.model_name);
        let path = transcribe::Transcriber::get_model_path(&settings.model_name);
        serde_json::json!({
            "model_name": settings.model_name,
            "exists": exists,
            "path": path.to_string_lossy(),
        })
    }
}

#[tauri::command]
async fn download_model(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let settings = state.controller.get_settings();
    if settings.model_name == "parakeet-tdt-0.6b-v3" {
        let path = parakeet::ParakeetTranscriber::ensure_model(&app_handle).await?;
        Ok(path.to_string_lossy().to_string())
    } else {
        let path = transcribe::Transcriber::ensure_model(&app_handle, &settings.model_name).await?;
        Ok(path.to_string_lossy().to_string())
    }
}

#[tauri::command]
fn get_audio_devices() -> serde_json::Value {
    serde_json::json!({
        "input_devices": audio::list_input_devices(),
        "output_devices": audio::list_output_devices(),
    })
}

#[tauri::command]
fn set_audio_device(
    state: State<'_, AppState>,
    kind: String,
    name: String,
) -> Result<(), String> {
    state.controller.set_audio_device(&kind, &name)
}

#[tauri::command]
fn open_preferences_window(app_handle: AppHandle) {
    tray::open_preferences(&app_handle);
}

#[tauri::command]
fn start_mic_test(app_handle: AppHandle, device_name: Option<String>) -> Result<(), String> {
    audio::start_mic_test(app_handle, device_name)
}

#[tauri::command]
fn stop_mic_test() -> Result<(), String> {
    audio::stop_mic_test();
    Ok(())
}

#[tauri::command]
fn play_test_sound() -> Result<(), String> {
    sound::play_sound(sound::AppSound::StartRecording);
    Ok(())
}

#[tauri::command]
fn is_model_loaded(state: State<'_, AppState>) -> bool {
    state.controller.is_model_loaded()
}

#[tauri::command]
fn eject_model(state: State<'_, AppState>) -> Result<(), String> {
    state.controller.eject_model();
    Ok(())
}

#[tauri::command]
fn get_tray_state(state: State<'_, AppState>) -> serde_json::Value {
    let loaded = state.controller.is_model_loaded();
    let phase = { state.controller.get_phase() };
    serde_json::json!({
        "is_loaded": loaded,
        "phase": phase,
        "can_eject": loaded,
    })
}

#[tauri::command]
fn close_preferences_window(app_handle: AppHandle) {
    audio::stop_mic_test();
    if let Some(win) = app_handle.get_webview_window("preferences") {
        let _ = win.hide();
    }
}

#[tauri::command]
fn save_window_position(state: State<'_, AppState>, x: i32, y: i32) -> Result<(), String> {
    let mut settings = state.controller.get_settings();
    settings.window_x = Some(x);
    settings.window_y = Some(y);
    state.controller.save_settings(settings)
}

#[tauri::command]
fn start_dragging_window(window: tauri::WebviewWindow) -> Result<(), String> {
    if fullscreen_pill::start_dragging() {
        return Ok(());
    }
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_transcription_logs(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Vec<pipeline_logger::TranscriptionDiagnosticLog> {
    state.controller.get_diagnostic_logs(limit.unwrap_or(50))
}

#[tauri::command]
fn clear_transcription_logs(state: State<'_, AppState>) -> Result<(), String> {
    state.controller.clear_diagnostic_logs();
    Ok(())
}

#[tauri::command]
fn get_available_models() -> Vec<transcribe::ModelCatalogItem> {
    transcribe::get_model_catalog()
}

#[tauri::command]
async fn download_specific_model(
    app_handle: AppHandle,
    model_name: String,
) -> Result<String, String> {
    if model_name == "parakeet-tdt-0.6b-v3" {
        let path = parakeet::ParakeetTranscriber::ensure_model(&app_handle).await?;
        Ok(path.to_string_lossy().to_string())
    } else {
        let path = transcribe::Transcriber::ensure_model(&app_handle, &model_name).await?;
        Ok(path.to_string_lossy().to_string())
    }
}

#[tauri::command]
fn get_audio_data(
    state: State<'_, AppState>,
    filename: Option<String>,
) -> Result<String, String> {
    state.controller.get_audio_data(filename)
}

#[tauri::command]
fn open_audio_folder(state: State<'_, AppState>) -> Result<(), String> {
    state.controller.open_audio_folder()
}

#[tauri::command]
fn play_recorded_audio(
    state: State<'_, AppState>,
    filename: Option<String>,
) -> Result<(), String> {
    state.controller.play_recorded_audio(filename)
}

#[tauri::command]
fn list_lab_audio_files(state: State<'_, AppState>) -> Vec<lab::LabAudioItem> {
    state.controller.list_lab_audio()
}

#[tauri::command]
async fn run_lab_experiment(
    state: State<'_, AppState>,
    req: lab::LabExperimentRequest,
) -> Result<lab::LabExperimentResult, String> {
    state.controller.run_lab_experiment(req).await
}

#[tauri::command]
fn save_lab_custom_audio(
    state: State<'_, AppState>,
    base64_wav: String,
) -> Result<String, String> {
    state.controller.save_lab_custom_audio(base64_wav)
}

#[tauri::command]
fn get_hardware_profile() -> cpu_features::HardwareProfile {
    cpu_features::detect_hardware_profile()
}

#[tauri::command]
fn post_process_transcript(
    state: State<'_, AppState>,
    text: String,
) -> post_processor::PostProcessedTranscript {
    let settings = state.controller.get_settings();
    let config = post_processor::PostProcessorConfig {
        normalization: text_normalizer::NormalizationOptions {
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
    post_processor::post_process(&text, config)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if let Some(state) = app.try_state::<AppState>() {
                            let controller = Arc::clone(&state.controller);
                            let mode = shortcut_mode(shortcut, &controller.get_settings());
                            let trigger = match mode {
                                RecordingMode::Main => "keyboard shortcut",
                                RecordingMode::Translate => "translate keyboard shortcut",
                            };
                            tauri::async_runtime::spawn(async move {
                                let _ = controller.toggle_recording_as(trigger, mode);
                            });
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            sound::preload();
            perf::init();

            let hw = cpu_features::detect_hardware_profile();
            log::info!("Hardware profile detected: {}", hw.summary);
            pipeline_logger::log_stage_event(&settings::get_data_dir(), "HARDWARE_DETECT", &hw.summary);

            let controller = Arc::new(AppController::new(app.handle().clone())?);
            let settings = controller.get_settings();
            let hotkey_str = settings.hotkey.clone();

            // Create macOS menu bar tray icon
            let _ = tray::create_tray(&app.handle(), &settings);
            crate::tray::update_tray_model_status(&app.handle(), false);
            updater::spawn_background_checks(app.handle());
            model_download::spawn_initial_download(app.handle(), settings.model_name.clone());

            // Configure window collection behavior and restore saved position if valid
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_always_on_top(true);
                let mut valid_coords = false;
                if let (Some(x), Some(y)) = (settings.window_x, settings.window_y) {
                    if let Ok(monitors) = win.available_monitors() {
                        for monitor in monitors {
                            let m_size = monitor.size();
                            let m_pos = monitor.position();
                            if x >= m_pos.x && (x + 100) <= (m_pos.x + m_size.width as i32)
                                && y >= m_pos.y && (y + 50) <= (m_pos.y + m_size.height as i32) {
                                let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }));
                                valid_coords = true;
                                break;
                            }
                        }
                    }
                }
                if !valid_coords {
                    let _ = win.center();
                }

                let _ = win.set_shadow(false);
                app_controller::apply_pill_window_behavior(&win, false);
            }

            // Return the pill to its own window once it goes idle, if it was moved into the
            // full-screen panel (see fullscreen_pill.rs).
            {
                let app_handle = app.handle().clone();
                app.listen("assistant-state-changed", move |event| {
                    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                        let state = payload.get("state").and_then(|v| v.as_str()).unwrap_or("idle");
                        fullscreen_pill::on_state_changed(&app_handle, state);
                    }
                });
            }

            let key_listener = global_key_listener::start_global_key_listener(
                Arc::clone(&controller),
                &hotkey_str,
                settings.active_translate_hotkey(),
                app.handle().clone(),
            );

            prompt_accessibility_for_modifier_hotkeys(&settings);

            app.manage(AppState {
                controller,
                key_listener,
            });

            register_global_shortcuts(app.handle(), &settings);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            toggle_recording,
            cancel_recording,
            retry_translation,
            paste_original,
            check_accessibility,
            request_accessibility,
            open_accessibility_settings,
            get_history,
            clear_history,
            get_model_status,
            download_model,
            get_audio_devices,
            set_audio_device,
            open_preferences_window,
            close_preferences_window,
            save_window_position,
            start_dragging_window,
            get_transcription_logs,
            clear_transcription_logs,
            get_available_models,
            download_specific_model,
            get_audio_data,
            open_audio_folder,
            play_recorded_audio,
            list_lab_audio_files,
            run_lab_experiment,
            save_lab_custom_audio,
            get_hardware_profile,
            post_process_transcript,
            start_mic_test,
            stop_mic_test,
            play_test_sound,
            custom_models::open_models_folder,
            custom_models::get_models_dir_path,
            custom_models::import_model_file,
            custom_models::pick_and_import_model,
            custom_models::download_custom_model,
            is_model_loaded,
            eject_model,
            get_tray_state,
            updater::get_update_status,
            updater::check_for_updates,
            updater::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
