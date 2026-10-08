#[allow(unused_imports)]
use std::sync::atomic::{AtomicU16, Ordering};
#[allow(unused_imports)]
use std::sync::Arc;

pub fn hotkey_str_to_modifier_keycode(s: &str) -> Option<u16> {
    let clean = s.trim().to_lowercase().replace(' ', "");
    match clean.as_str() {
        "rightoption" | "rightalt" | "altright" | "optionright" => Some(61),
        "rightcontrol" | "rightctrl" | "controlright" | "ctrlright" => Some(62),
        "leftoption" | "leftalt" | "altleft" | "optionleft" => Some(58),
        "leftcontrol" | "leftctrl" | "controlleft" | "ctrlleft" => Some(59),
        "rightcommand" | "rightcmd" | "metaright" | "commandright" => Some(54),
        "leftcommand" | "leftcmd" | "metaleft" | "commandleft" => Some(55),
        "rightshift" | "shiftright" => Some(60),
        "leftshift" | "shiftleft" => Some(56),
        _ => None,
    }
}

/// Modifier keycodes for the main and translate hotkeys (0 = not a modifier-only key). The
/// translate hotkey is dropped when it's the same key as the main one.
#[allow(dead_code)]
fn modifier_keycodes(hotkey: &str, translate_hotkey: &str) -> (u16, u16) {
    let main = hotkey_str_to_modifier_keycode(hotkey).unwrap_or(0);
    let translate = hotkey_str_to_modifier_keycode(translate_hotkey).unwrap_or(0);
    (main, if translate == main { 0 } else { translate })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_mappings() {
        assert_eq!(hotkey_str_to_modifier_keycode("RightOption"), Some(61));
        assert_eq!(hotkey_str_to_modifier_keycode("RightAlt"), Some(61));
        assert_eq!(hotkey_str_to_modifier_keycode("AltRight"), Some(61));
        assert_eq!(hotkey_str_to_modifier_keycode("RightControl"), Some(62));
        assert_eq!(hotkey_str_to_modifier_keycode("RightCtrl"), Some(62));
        assert_eq!(hotkey_str_to_modifier_keycode("ControlRight"), Some(62));
        assert_eq!(hotkey_str_to_modifier_keycode("LeftOption"), Some(58));
        assert_eq!(hotkey_str_to_modifier_keycode("LeftControl"), Some(59));
        assert_eq!(hotkey_str_to_modifier_keycode("CommandOrControl+Shift+Space"), None);
    }

    #[test]
    fn test_modifier_keycodes() {
        assert_eq!(modifier_keycodes("RightOption", "RightControl"), (61, 62));
        assert_eq!(modifier_keycodes("RightOption", ""), (61, 0));
        assert_eq!(modifier_keycodes("RightOption", "AltRight"), (61, 0));
        assert_eq!(modifier_keycodes("Control+Shift+Space", "RightCommand"), (0, 54));
    }
}

#[cfg(target_os = "macos")]
pub mod macos {
    use super::*;
    use crate::app_controller::{AppController, RecordingMode};
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Instant;

    type CGEventTapProxy = *mut c_void;
    type CGEventType = u32;
    type CGEventRef = *mut c_void;
    type CFMachPortRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *mut c_void;

    #[allow(non_upper_case_globals)]
    const kCGEventKeyDown: u32 = 10;
    #[allow(non_upper_case_globals)]
    const kCGEventFlagsChanged: u32 = 12;

    #[allow(non_upper_case_globals)]
    const kCGHIDEventTap: u32 = 0;
    #[allow(non_upper_case_globals)]
    const kCGSessionEventTap: u32 = 1;
    #[allow(non_upper_case_globals)]
    const kCGHeadInsertEventTap: u32 = 0;
    #[allow(non_upper_case_globals)]
    const kCGEventTapOptionListenOnly: u32 = 1;

    #[allow(non_upper_case_globals)]
    const kCGKeyboardEventKeycode: u32 = 9;

    #[allow(non_upper_case_globals)]
    const kCGEventTapDisabledByTimeout: u32 = 0xFFFFFFFE;
    #[allow(non_upper_case_globals)]
    const kCGEventTapDisabledByUserInput: u32 = 0xFFFFFFFF;

    #[allow(non_upper_case_globals)]
    const kCGEventFlagMaskAlternate: u64 = 0x00080000;
    #[allow(non_upper_case_globals)]
    const kCGEventFlagMaskControl: u64 = 0x00040000;
    #[allow(non_upper_case_globals)]
    const kCGEventFlagMaskCommand: u64 = 0x00100000;
    #[allow(non_upper_case_globals)]
    const kCGEventFlagMaskShift: u64 = 0x00020000;

    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: unsafe extern "C" fn(
                proxy: CGEventTapProxy,
                r#type: CGEventType,
                event: CGEventRef,
                user_info: *mut c_void,
            ) -> CGEventRef,
            user_info: *mut c_void,
        ) -> CFMachPortRef;

        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;
        fn CFMachPortIsValid(port: CFMachPortRef) -> bool;

        fn CFMachPortCreateRunLoopSource(
            allocator: *mut c_void,
            port: CFMachPortRef,
            order: isize,
        ) -> CFRunLoopSourceRef;

        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
        fn CFRunLoopRun();
        fn CFRunLoopStop(rl: CFRunLoopRef);

        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        fn CGEventGetFlags(event: CGEventRef) -> u64;

        static kCFRunLoopCommonModes: CFStringRef;
        static kCFRunLoopDefaultMode: CFStringRef;

        fn AXIsProcessTrusted() -> u8;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
        static kAXTrustedCheckOptionPrompt: *const c_void;
        static kCFBooleanTrue: *const c_void;
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> *const c_void;
        fn CFRelease(cf: *const c_void);
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
    }

    pub fn is_accessibility_trusted() -> bool {
        unsafe { AXIsProcessTrusted() != 0 }
    }

    pub fn request_accessibility_prompt() {
        if is_accessibility_trusted() {
            log::info!("Accessibility is already trusted. Opening settings only.");
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                .spawn();
            return;
        }

        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let dict = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks as *const _ as *const _,
                &kCFTypeDictionaryValueCallBacks as *const _ as *const _,
            );
            let _ = AXIsProcessTrustedWithOptions(dict);
            if !dict.is_null() {
                CFRelease(dict);
            }
        }
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }

    struct KeyListenerState {
        controller: Arc<AppController>,
        target_keycode: Arc<AtomicU16>,
        translate_keycode: Arc<AtomicU16>,
        /// The hotkey currently held down (0 = none), so only its release ends hold-to-talk.
        down_keycode: AtomicU16,
        tap_port: Mutex<Option<CFMachPortRef>>,
        last_down_time: Mutex<Option<Instant>>,
        is_key_down: AtomicBool,
        run_loop: Mutex<Option<CFRunLoopRef>>,
    }

    unsafe impl Send for KeyListenerState {}
    unsafe impl Sync for KeyListenerState {}

    impl KeyListenerState {
        fn handle_modifier_down(&self, keycode: u16, mode: RecordingMode) {
            let was_down = self.is_key_down.swap(true, Ordering::SeqCst);
            if was_down {
                // If the key was held for longer than 2 seconds, it was stuck by sleep or a missed release event.
                let is_stale = {
                    let guard = self.last_down_time.lock().unwrap();
                    guard.map(|t| t.elapsed().as_secs() >= 2).unwrap_or(false)
                };
                if !is_stale {
                    return; // Debounce repeat flag events
                }
                log::warn!("Detected stuck modifier key from missed release event. Resetting state.");
            }
            {
                let mut down_time = self.last_down_time.lock().unwrap();
                *down_time = Some(Instant::now());
            }
            self.down_keycode.store(keycode, Ordering::SeqCst);

            let trigger = match mode {
                RecordingMode::Main => "hotkey press",
                RecordingMode::Translate => "translate hotkey press",
            };
            let controller = Arc::clone(&self.controller);
            tauri::async_runtime::spawn(async move {
                let _ = controller.toggle_recording_as(trigger, mode);
            });
        }

        fn handle_modifier_up(&self, keycode: u16) {
            // Releasing the other hotkey while one is held doesn't end anything.
            if self.down_keycode.load(Ordering::SeqCst) != keycode {
                return;
            }
            let was_down = self.is_key_down.swap(false, Ordering::SeqCst);
            if !was_down {
                return;
            }
            self.down_keycode.store(0, Ordering::SeqCst);

            let elapsed = {
                let mut down_time = self.last_down_time.lock().unwrap();
                down_time.take().map(|t| t.elapsed()).unwrap_or_default()
            };

            // If the user held the key for longer than 350ms (hold-to-talk gesture) and under 10 seconds,
            // and the assistant is currently listening, release stops recording!
            if elapsed.as_millis() >= 350 && elapsed.as_secs() < 10 && self.controller.is_listening() {
                let controller = Arc::clone(&self.controller);
                tauri::async_runtime::spawn(async move {
                    let _ = controller.toggle_recording("hotkey release (hold-to-talk)");
                });
            }
        }
    }

    unsafe extern "C" fn event_tap_callback(
        _proxy: CGEventTapProxy,
        event_type: CGEventType,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef {
        if user_info.is_null() {
            return event;
        }

        let state = &*(user_info as *const KeyListenerState);

        // Re-enable tap if disabled by system timeout or user input
        if event_type == kCGEventTapDisabledByTimeout || event_type == kCGEventTapDisabledByUserInput {
            log::warn!("CGEventTap disabled by system (type {}), re-enabling...", event_type);
            if let Ok(guard) = state.tap_port.lock() {
                if let Some(port) = *guard {
                    CGEventTapEnable(port, true);
                }
            }
            return event;
        }

        let keycode = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode) as u16;

        if event_type == kCGEventKeyDown {
            // Escape key code = 53
            if keycode == 53 {
                if state.controller.is_active() {
                    log::info!("Global Esc detected. Cancelling recording or transcription...");
                    let controller = Arc::clone(&state.controller);
                    tauri::async_runtime::spawn(async move {
                        controller.cancel("Esc key");
                    });
                }
            }
        } else if event_type == kCGEventFlagsChanged {
            let target = state.target_keycode.load(Ordering::SeqCst);
            let translate = state.translate_keycode.load(Ordering::SeqCst);
            let mode = if target != 0 && keycode == target {
                Some(RecordingMode::Main)
            } else if translate != 0 && keycode == translate {
                Some(RecordingMode::Translate)
            } else {
                None
            };
            if let Some(mode) = mode {
                let flags = CGEventGetFlags(event);
                let is_down = match keycode {
                    61 | 58 => (flags & kCGEventFlagMaskAlternate) != 0,
                    62 | 59 => (flags & kCGEventFlagMaskControl) != 0,
                    54 | 55 => (flags & kCGEventFlagMaskCommand) != 0,
                    60 | 56 => (flags & kCGEventFlagMaskShift) != 0,
                    _ => false,
                };

                log::info!("Global modifier keycode {} changed: is_down={}", keycode, is_down);

                if is_down {
                    state.handle_modifier_down(keycode, mode);
                } else {
                    state.handle_modifier_up(keycode);
                }
            }
        }

        event
    }

    pub struct GlobalKeyListenerHandle {
        pub target_keycode: Arc<AtomicU16>,
        pub translate_keycode: Arc<AtomicU16>,
    }

    impl GlobalKeyListenerHandle {
        pub fn update_hotkeys(&self, hotkey_str: &str, translate_hotkey_str: &str) {
            let (code, translate_code) = modifier_keycodes(hotkey_str, translate_hotkey_str);
            self.target_keycode.store(code, Ordering::SeqCst);
            self.translate_keycode.store(translate_code, Ordering::SeqCst);
            log::info!("Updated global modifier keycodes: main {}, translate {}", code, translate_code);
        }
    }

    pub fn start_global_key_listener(
        controller: Arc<AppController>,
        initial_hotkey: &str,
        initial_translate_hotkey: &str,
        app_handle: tauri::AppHandle,
    ) -> Arc<GlobalKeyListenerHandle> {
        let (target_code, translate_code) = modifier_keycodes(initial_hotkey, initial_translate_hotkey);
        let target_keycode = Arc::new(AtomicU16::new(target_code));
        let translate_keycode = Arc::new(AtomicU16::new(translate_code));
        let handle = Arc::new(GlobalKeyListenerHandle {
            target_keycode: Arc::clone(&target_keycode),
            translate_keycode: Arc::clone(&translate_keycode),
        });

        let state = Arc::new(KeyListenerState {
            controller,
            target_keycode: Arc::clone(&target_keycode),
            translate_keycode: Arc::clone(&translate_keycode),
            down_keycode: AtomicU16::new(0),
            tap_port: Mutex::new(None),
            last_down_time: Mutex::new(None),
            is_key_down: AtomicBool::new(false),
            run_loop: Mutex::new(None),
        });

        // 1. Thread for CGEventTap and CFRunLoop
        let state_runloop = Arc::clone(&state);
        thread::Builder::new()
            .name("revfly-global-keys".to_string())
            .spawn(move || {
                loop {
                    // Wait for Accessibility trust if needed
                    let mut logged_warning = false;
                    while !is_accessibility_trusted() {
                        if !logged_warning {
                            log::warn!("Accessibility is not trusted yet. Global modifier key listener waiting for grant.");
                            logged_warning = true;
                        }
                        thread::sleep(std::time::Duration::from_millis(1000));
                    }
                    if logged_warning {
                        log::info!("Accessibility permission confirmed. Starting CGEventTap...");
                    }

                    // Event mask: key down (for ESC) + flags changed (for modifiers)
                    let mask = (1u64 << kCGEventKeyDown) | (1u64 << kCGEventFlagsChanged);
                    let state_raw = Arc::as_ptr(&state_runloop) as *mut c_void;
                    let mut tap = unsafe {
                        CGEventTapCreate(
                            kCGHIDEventTap,
                            kCGHeadInsertEventTap,
                            kCGEventTapOptionListenOnly,
                            mask,
                            event_tap_callback,
                            state_raw,
                        )
                    };

                    if tap.is_null() {
                        tap = unsafe {
                            CGEventTapCreate(
                                kCGSessionEventTap,
                                kCGHeadInsertEventTap,
                                kCGEventTapOptionListenOnly,
                                mask,
                                event_tap_callback,
                                state_raw,
                            )
                        };
                    }

                    if tap.is_null() {
                        log::warn!("Failed to create CGEventTap. Retrying in 2 seconds...");
                        thread::sleep(std::time::Duration::from_millis(2000));
                        continue;
                    }

                    unsafe {
                        if let Ok(mut guard) = state_runloop.tap_port.lock() {
                            *guard = Some(tap);
                        }

                        let source = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
                        let rl = CFRunLoopGetCurrent();
                        if let Ok(mut rl_guard) = state_runloop.run_loop.lock() {
                            *rl_guard = Some(rl);
                        }

                        CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
                        CFRunLoopAddSource(rl, source, kCFRunLoopDefaultMode);
                        CGEventTapEnable(tap, true);
                        log::info!("Global key listener runloop active and listening for hotkeys.");
                        CFRunLoopRun();

                        log::warn!("Global key listener runloop stopped. Re-creating event tap...");
                        if let Ok(mut guard) = state_runloop.tap_port.lock() {
                            *guard = None;
                        }
                        if let Ok(mut rl_guard) = state_runloop.run_loop.lock() {
                            *rl_guard = None;
                        }
                        if !source.is_null() {
                            CFRelease(source);
                        }
                        if !tap.is_null() {
                            CFRelease(tap);
                        }
                    }

                    thread::sleep(std::time::Duration::from_millis(500));
                }
            })
            .expect("Failed to spawn global keys thread");

        // 2. Watchdog thread for sleep/wake recovery and event tap health
        let state_watchdog = Arc::clone(&state);
        thread::Builder::new()
            .name("revfly-sleep-watchdog".to_string())
            .spawn(move || {
                let mut last_tick = Instant::now();
                loop {
                    thread::sleep(std::time::Duration::from_millis(1000));
                    let elapsed = last_tick.elapsed();
                    last_tick = Instant::now();

                    let woke_from_sleep = elapsed.as_millis() > 2500;
                    if woke_from_sleep {
                        log::info!(
                            "System woke from sleep (tick gap: {:?}). Resetting keys and verifying event tap...",
                            elapsed
                        );
                        // Reset key down state
                        state_watchdog.is_key_down.store(false, Ordering::SeqCst);
                        state_watchdog.down_keycode.store(0, Ordering::SeqCst);
                        if let Ok(mut dt) = state_watchdog.last_down_time.lock() {
                            *dt = None;
                        }

                        // Re-register global shortcuts for Carbon (tauri plugin)
                        let settings = state_watchdog.controller.get_settings();
                        crate::register_global_shortcuts(&app_handle, &settings);
                    }

                    // Check CGEventTap validity and state
                    if is_accessibility_trusted() {
                        if let Ok(guard) = state_watchdog.tap_port.lock() {
                            if let Some(port) = *guard {
                                unsafe {
                                    if CFMachPortIsValid(port) {
                                        if !CGEventTapIsEnabled(port) {
                                            log::warn!("Watchdog: CGEventTap disabled, re-enabling now.");
                                            CGEventTapEnable(port, true);
                                        }
                                    } else {
                                        log::warn!("Watchdog: CFMachPort invalid after sleep/disconnect. Stopping runloop to recreate.");
                                        if let Ok(rl_guard) = state_watchdog.run_loop.lock() {
                                            if let Some(rl) = *rl_guard {
                                                CFRunLoopStop(rl);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .expect("Failed to spawn sleep watchdog thread");

        handle
    }
}

#[cfg(not(target_os = "macos"))]
pub mod non_macos {
    use super::*;
    use crate::app_controller::AppController;
    use std::sync::atomic::{AtomicU16, Ordering};
    use std::sync::Arc;

    pub fn is_accessibility_trusted() -> bool {
        true
    }

    pub fn request_accessibility_prompt() {}

    /// Windows and Linux have no modifier-only hotkeys; both hotkeys are key combos registered
    /// through tauri-plugin-global-shortcut (see `register_global_shortcuts`).
    pub struct GlobalKeyListenerHandle {
        pub target_keycode: Arc<AtomicU16>,
    }

    impl GlobalKeyListenerHandle {
        pub fn update_hotkeys(&self, _hotkey_str: &str, _translate_hotkey_str: &str) {}
    }

    pub fn start_global_key_listener(
        _controller: Arc<AppController>,
        _initial_hotkey: &str,
        _initial_translate_hotkey: &str,
        _app_handle: tauri::AppHandle,
    ) -> Arc<GlobalKeyListenerHandle> {
        Arc::new(GlobalKeyListenerHandle {
            target_keycode: Arc::new(AtomicU16::new(0)),
        })
    }
}

#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(not(target_os = "macos"))]
pub use non_macos::*;

