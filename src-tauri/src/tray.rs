use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};

use crate::audio::{list_input_devices, list_output_devices};
use crate::settings::AppSettings;

pub fn open_preferences(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("preferences") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        let mut builder = WebviewWindowBuilder::new(
            app,
            "preferences",
            WebviewUrl::App("index.html#preferences".into()),
        )
        .title("Settings")
        .inner_size(880.0, 580.0)
        .min_inner_size(680.0, 480.0)
        .resizable(true)
        .devtools(true)
        .center();

        #[cfg(target_os = "macos")]
        {
            builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
        }

        if let Ok(win) = builder.build() {
            let win_clone = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = win_clone.hide();
                    crate::audio::stop_mic_test();
                }
            });
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayIconState {
    Ejected,
    Loaded,
    Recording,
}

/// Sleeping-state tray icon. macOS gets a white template image that the system tints to match the
/// menu bar; Windows and Linux draw icons as-is, so a white glyph vanishes on light taskbars and they
/// get the full-color app icon instead.
#[cfg(target_os = "macos")]
const SLEEPING_ICON: &[u8] = include_bytes!("../icons/tray-ejected@2x.png");
#[cfg(not(target_os = "macos"))]
const SLEEPING_ICON: &[u8] = include_bytes!("../icons/32x32.png");
const SLEEPING_ICON_IS_TEMPLATE: bool = cfg!(target_os = "macos");

pub fn update_tray_icon(app: &AppHandle, state: TrayIconState) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let (icon_bytes, is_template, title): (&[u8], bool, Option<&str>) = match state {
            TrayIconState::Recording => (include_bytes!("../icons/tray-recording@2x.png"), false, Some(" REC")),
            TrayIconState::Loaded => (include_bytes!("../icons/tray-loaded@2x.png"), false, Some("")),
            TrayIconState::Ejected => (SLEEPING_ICON, SLEEPING_ICON_IS_TEMPLATE, Some("")),
        };
        if let Ok(icon) = tauri::image::Image::from_bytes(icon_bytes) {
            let _ = tray.set_icon(Some(icon));
            let _ = tray.set_icon_as_template(is_template);
        }
        let tip = match state {
            TrayIconState::Recording => "RevFly — Recording in progress…",
            TrayIconState::Loaded => "RevFly — Speech Model Active in RAM",
            TrayIconState::Ejected => "RevFly — Sleeping (RAM Saved)",
        };
        let _ = tray.set_tooltip(Some(tip));
        #[cfg(target_os = "macos")]
        let _ = tray.set_title(title);
    }
}

static STATUS_ITEM: std::sync::OnceLock<tauri::menu::MenuItem<tauri::Wry>> = std::sync::OnceLock::new();
static EJECT_ITEM: std::sync::OnceLock<tauri::menu::MenuItem<tauri::Wry>> = std::sync::OnceLock::new();
static TOGGLE_ITEM: std::sync::OnceLock<tauri::menu::MenuItem<tauri::Wry>> = std::sync::OnceLock::new();
static UPDATE_ITEM: std::sync::OnceLock<tauri::menu::MenuItem<tauri::Wry>> = std::sync::OnceLock::new();
static RETRY_ITEM: std::sync::OnceLock<tauri::menu::MenuItem<tauri::Wry>> = std::sync::OnceLock::new();
static IS_RECORDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn update_tray_model_status(app: &AppHandle, is_loaded: bool) {
    // If user is actively recording, preserve the Recording icon & REC title
    if !IS_RECORDING.load(std::sync::atomic::Ordering::SeqCst) {
        update_tray_icon(app, if is_loaded { TrayIconState::Loaded } else { TrayIconState::Ejected });
    }
    if let Some(item) = STATUS_ITEM.get() {
        let label = if is_loaded {
            "● Speech Model: Active (In RAM) — tap to eject"
        } else {
            "○ Speech Model: Sleeping (RAM Saved)"
        };
        let _ = item.set_text(label);
        let _ = item.set_enabled(is_loaded);
    }
    if let Some(item) = EJECT_ITEM.get() {
        let _ = item.set_enabled(is_loaded && !IS_RECORDING.load(std::sync::atomic::Ordering::SeqCst));
        let _ = item.set_text(if is_loaded { "Eject Model from RAM" } else { "Model Ejected (Sleeping)" });
    }
    let _ = app; // silence unused if no tray yet
}

pub fn set_model_download_progress(percent: u32) {
    if let Some(item) = STATUS_ITEM.get() {
        let _ = item.set_text(format!("↓ Downloading Speech Model… {}%", percent));
        let _ = item.set_enabled(false);
    }
}

/// Enables "Retry Last Translation" while a failed translation is waiting to be retried.
pub fn set_retry_item(enabled: bool) {
    if let Some(item) = RETRY_ITEM.get() {
        let _ = item.set_enabled(enabled);
    }
}

pub fn is_recording() -> bool {
    IS_RECORDING.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn set_update_item(_app: &AppHandle, label: &str, enabled: bool) {
    if let Some(item) = UPDATE_ITEM.get() {
        let _ = item.set_text(label);
        let _ = item.set_enabled(enabled);
    }
}

pub fn set_tray_recording(app: &AppHandle, is_recording: bool, llm_loaded: bool) {
    IS_RECORDING.store(is_recording, std::sync::atomic::Ordering::SeqCst);
    if is_recording {
        update_tray_icon(app, TrayIconState::Recording);
        if let Some(item) = TOGGLE_ITEM.get() {
            let _ = item.set_text("Stop Recording");
        }
        // While recording, keep menu items reflective but not ejectable mid-capture
        if let Some(item) = EJECT_ITEM.get() {
            let _ = item.set_enabled(false);
        }
    } else {
        if let Some(item) = TOGGLE_ITEM.get() {
            let _ = item.set_text("Start Recording");
        }
        update_tray_model_status(app, llm_loaded);
    }
}

pub fn create_tray(app: &AppHandle, settings: &AppSettings) -> Result<TrayIcon, tauri::Error> {
    // Cheap RAM check at tray creation: any model loaded in this process? Check via controller if available later
    // Initial label: assume ejected; will be corrected immediately by update_tray_model_status after startup
    let status_item = MenuItem::with_id(
        app,
        "model_status",
        "○ Speech Model: Sleeping (RAM Saved)",
        false,
        None::<&str>,
    )?;
    let eject_item = MenuItem::with_id(
        app,
        "eject_model",
        "Eject Model from RAM",
        false,
        None::<&str>,
    )?;
    let _ = STATUS_ITEM.set(status_item.clone());
    let _ = EJECT_ITEM.set(eject_item.clone());
    let sep0 = PredefinedMenuItem::separator(app)?;

    let toggle_item = MenuItem::with_id(app, "toggle_recording", "Start Recording", true, None::<&str>)?;
    let _ = TOGGLE_ITEM.set(toggle_item.clone());
    // Enabled while the last translation failed and can be sent again.
    let retry_item = MenuItem::with_id(app, "retry_translation", "Retry Last Translation", false, None::<&str>)?;
    let _ = RETRY_ITEM.set(retry_item.clone());
    let sep1 = PredefinedMenuItem::separator(app)?;

    // Microphone Input submenu
    let input_devices = list_input_devices();
    let current_input = settings.input_device.as_deref().unwrap_or("Default");
    let mut input_menu_items = Vec::new();

    let def_input = CheckMenuItem::with_id(
        app,
        "input_dev:Default",
        "System Default",
        true,
        current_input == "Default",
        None::<&str>,
    )?;
    input_menu_items.push(def_input);

    for (_idx, dev) in input_devices.iter().enumerate() {
        let is_checked = current_input == dev;
        let item = CheckMenuItem::with_id(
            app,
            format!("input_dev:{}", dev),
            dev,
            true,
            is_checked,
            None::<&str>,
        )?;
        input_menu_items.push(item);
    }

    let input_items_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = input_menu_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    let input_submenu = Submenu::with_items(app, "Microphone Input", true, &input_items_refs)?;

    // Audio Output submenu
    let output_devices = list_output_devices();
    let current_output = settings.output_device.as_deref().unwrap_or("Default");
    let mut output_menu_items = Vec::new();

    let def_output = CheckMenuItem::with_id(
        app,
        "output_dev:Default",
        "System Default",
        true,
        current_output == "Default",
        None::<&str>,
    )?;
    output_menu_items.push(def_output);

    for (_idx, dev) in output_devices.iter().enumerate() {
        let is_checked = current_output == dev;
        let item = CheckMenuItem::with_id(
            app,
            format!("output_dev:{}", dev),
            dev,
            true,
            is_checked,
            None::<&str>,
        )?;
        output_menu_items.push(item);
    }

    let output_items_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = output_menu_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    let output_submenu = Submenu::with_items(app, "Audio Output", true, &output_items_refs)?;

    // Translation submenu: 3 options (No Translation, LLM [both local & cloud], Custom API)
    let current_provider = &settings.translation_provider;
    let is_no_trans = current_provider.eq_ignore_ascii_case("no translation")
        || current_provider.eq_ignore_ascii_case("none")
        || settings.target_lang == "No Translation";
    let is_custom_api = current_provider.to_lowercase().contains("custom") && !is_no_trans;
    let is_llm = !is_no_trans && !is_custom_api;

    let mut trans_menu_items = Vec::new();

    let no_trans_item = CheckMenuItem::with_id(
        app,
        "trans_provider:No Translation",
        "No Translation (Voice to Text only)",
        true,
        is_no_trans,
        None::<&str>,
    )?;
    trans_menu_items.push(no_trans_item);

    let llm_item = CheckMenuItem::with_id(
        app,
        "trans_provider:LLM",
        "LLM (both local & cloud)",
        true,
        is_llm,
        None::<&str>,
    )?;
    trans_menu_items.push(llm_item);

    let custom_api_item = CheckMenuItem::with_id(
        app,
        "trans_provider:Custom API",
        "Custom API",
        true,
        is_custom_api,
        None::<&str>,
    )?;
    trans_menu_items.push(custom_api_item);

    let trans_items_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = trans_menu_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    let trans_submenu = Submenu::with_items(app, "Translation", true, &trans_items_refs)?;

    let sep2 = PredefinedMenuItem::separator(app)?;
    let update_item = MenuItem::with_id(app, "check_updates", "Check for Updates…", true, None::<&str>)?;
    let _ = UPDATE_ITEM.set(update_item.clone());
    let pref_item = MenuItem::with_id(app, "preferences", "Settings...", true, None::<&str>)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit RevFly", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[
        &status_item,
        &eject_item,
        &sep0,
        &toggle_item,
        &retry_item,
        &sep1,
        &input_submenu,
        &output_submenu,
        &trans_submenu,
        &sep2,
        &update_item,
        &pref_item,
        &sep3,
        &quit_item,
    ])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("RevFly — LLM Ejected (RAM Saved)")
        .icon_as_template(SLEEPING_ICON_IS_TEMPLATE)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if id == "toggle_recording" {
                if let Some(state) = app.try_state::<crate::AppState>() {
                    let controller = std::sync::Arc::clone(&state.controller);
                    tauri::async_runtime::spawn(async move {
                        let _ = controller.toggle_recording("tray menu");
                    });
                }
            } else if id == "retry_translation" {
                if let Some(state) = app.try_state::<crate::AppState>() {
                    if let Err(e) = state.controller.retry_translation() {
                        log::warn!("Retry translation from tray: {}", e);
                    }
                }
            } else if id == "preferences" {
                open_preferences(app);
            } else if id == "check_updates" {
                crate::updater::on_tray_click(app);
            } else if id == "eject_model" {
                if let Some(state) = app.try_state::<crate::AppState>() {
                    state.controller.eject_model();
                }
            } else if id == "model_status" {
                // status is informational only; also trigger eject when loaded (tapping the status line)
                if let Some(state) = app.try_state::<crate::AppState>() {
                    if state.controller.is_model_loaded() {
                        state.controller.eject_model();
                    }
                }
            } else if id == "quit" {
                std::process::exit(0);
            } else if let Some(dev) = id.strip_prefix("input_dev:") {
                if let Some(state) = app.try_state::<crate::AppState>() {
                    let _ = state.controller.set_audio_device("input", dev);
                }
            } else if let Some(dev) = id.strip_prefix("output_dev:") {
                if let Some(state) = app.try_state::<crate::AppState>() {
                    let _ = state.controller.set_audio_device("output", dev);
                }
            } else if let Some(provider) = id.strip_prefix("trans_provider:") {
                if let Some(state) = app.try_state::<crate::AppState>() {
                    let _ = state.controller.set_translation_provider(provider);
                }
            }
        });

    if let Ok(icon) = tauri::image::Image::from_bytes(SLEEPING_ICON) {
        builder = builder.icon(icon);
    } else if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    let tray = builder.build(app)?;
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(Some(""));

    Ok(tray)
}
