//! Shows the voice pill over native macOS full-screen apps.
//!
//! A full-screen Space only composites windows that are real `NSPanel` instances; the pill window is a
//! plain `NSWindow`, and swapping its class with `object_setClass` breaks WebKit's KVO observers and
//! aborts the app (see `apply_pill_window_behavior`). So whenever the pill is shown, the pill window's
//! content view (the webview itself) is moved into a separately created `NSPanel` at the same frame,
//! and moved back when the pill goes away. Doing this always, rather than only when a full-screen app
//! is detected, avoids depending on Accessibility permission to detect it. The pill looks and behaves
//! the same; only the window hosting it changes.

use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;
    use std::sync::{Mutex, OnceLock};

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        fn objc_msgSend(receiver: *mut c_void, sel: *const c_void, ...) -> *mut c_void;
        fn objc_getClass(name: *const u8) -> *mut c_void;
        fn sel_registerName(name: *const u8) -> *const c_void;
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }

    /// The panel, plus the pill window and its content view while they're moved into the panel.
    struct Host {
        panel: *mut c_void,
        adopted: Option<(*mut c_void, *mut c_void)>,
    }
    // Only ever touched on the main thread (every caller runs inside `run_on_main_thread`).
    unsafe impl Send for Host {}

    static HOST: OnceLock<Mutex<Option<Host>>> = OnceLock::new();

    unsafe fn get(recv: *mut c_void, sel_name: &[u8]) -> *mut c_void {
        let send: extern "C" fn(*mut c_void, *const c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        send(recv, sel_registerName(sel_name.as_ptr()))
    }

    unsafe fn set_bool(recv: *mut c_void, sel_name: &[u8], value: bool) {
        let send: extern "C" fn(*mut c_void, *const c_void, bool) = std::mem::transmute(objc_msgSend as *const ());
        send(recv, sel_registerName(sel_name.as_ptr()), value);
    }

    unsafe fn set_usize(recv: *mut c_void, sel_name: &[u8], value: usize) {
        let send: extern "C" fn(*mut c_void, *const c_void, usize) = std::mem::transmute(objc_msgSend as *const ());
        send(recv, sel_registerName(sel_name.as_ptr()), value);
    }

    unsafe fn set_isize(recv: *mut c_void, sel_name: &[u8], value: isize) {
        let send: extern "C" fn(*mut c_void, *const c_void, isize) = std::mem::transmute(objc_msgSend as *const ());
        send(recv, sel_registerName(sel_name.as_ptr()), value);
    }

    unsafe fn set_ptr(recv: *mut c_void, sel_name: &[u8], value: *mut c_void) {
        let send: extern "C" fn(*mut c_void, *const c_void, *mut c_void) = std::mem::transmute(objc_msgSend as *const ());
        send(recv, sel_registerName(sel_name.as_ptr()), value);
    }

    unsafe fn call(recv: *mut c_void, sel_name: &[u8]) {
        let send: extern "C" fn(*mut c_void, *const c_void) = std::mem::transmute(objc_msgSend as *const ());
        send(recv, sel_registerName(sel_name.as_ptr()));
    }

    unsafe fn frame_of(win: *mut c_void) -> NSRect {
        let f: extern "C" fn(*mut c_void, *const c_void) -> NSRect = std::mem::transmute(objc_msgSend as *const ());
        f(win, sel_registerName(b"frame\0".as_ptr()))
    }

    unsafe fn set_frame(win: *mut c_void, frame: NSRect) {
        let f: extern "C" fn(*mut c_void, *const c_void, NSRect, bool) = std::mem::transmute(objc_msgSend as *const ());
        f(win, sel_registerName(b"setFrame:display:\0".as_ptr()), frame, true);
    }

    unsafe fn new_view(frame: NSRect) -> *mut c_void {
        let alloc = get(objc_getClass(b"NSView\0".as_ptr()), b"alloc\0");
        let init: extern "C" fn(*mut c_void, *const c_void, NSRect) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        init(alloc, sel_registerName(b"initWithFrame:\0".as_ptr()), frame)
    }

    /// A transparent, borderless, non-activating `NSPanel` created fresh (never class-swapped), with the
    /// same Space/level settings as the pill window. Being a real NSPanel is what lets it show over a
    /// native full-screen app.
    unsafe fn create_panel() -> *mut c_void {
        let panel_alloc = get(objc_getClass(b"NSPanel\0".as_ptr()), b"alloc\0");
        // NonactivatingPanel (1 << 7), NSBackingStoreBuffered (2), defer: NO
        type InitFn = extern "C" fn(*mut c_void, *const c_void, NSRect, usize, usize, bool) -> *mut c_void;
        let init: InitFn = std::mem::transmute(objc_msgSend as *const ());
        let panel = init(
            panel_alloc,
            sel_registerName(b"initWithContentRect:styleMask:backing:defer:\0".as_ptr()),
            NSRect { x: 0.0, y: 0.0, w: 340.0, h: 100.0 },
            1 << 7,
            2,
            false,
        );

        set_bool(panel, b"setFloatingPanel:\0", true);
        set_bool(panel, b"setBecomesKeyOnlyIfNeeded:\0", true);
        set_bool(panel, b"setHidesOnDeactivate:\0", false);
        set_bool(panel, b"setReleasedWhenClosed:\0", false);
        // The webview draws the pill (rounded shape, shadow) on a transparent background.
        set_bool(panel, b"setOpaque:\0", false);
        set_bool(panel, b"setHasShadow:\0", false);
        let clear = get(objc_getClass(b"NSColor\0".as_ptr()), b"clearColor\0");
        set_ptr(panel, b"setBackgroundColor:\0", clear);
        // NSWindowCollectionBehaviorCanJoinAllSpaces (1) | IgnoresCycle (64) | FullScreenAuxiliary (256)
        set_usize(panel, b"setCollectionBehavior:\0", (1 << 0) | (1 << 6) | (1 << 8));
        // NSScreenSaverWindowLevel
        set_isize(panel, b"setLevel:\0", 1000);
        panel
    }

    fn with_host<R, F: FnOnce(&mut Host) -> R>(f: F) -> R {
        let mutex = HOST.get_or_init(|| Mutex::new(None));
        let mut guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
        let host = guard.get_or_insert_with(|| Host { panel: unsafe { create_panel() }, adopted: None });
        f(host)
    }

    /// Moves the pill window's content view (the webview) into the panel, at the window's exact
    /// frame, and hides the window. Only the view moves; the window's class is never touched.
    pub fn adopt(ns_window: *mut c_void) {
        with_host(|host| unsafe {
            if host.adopted.is_none() {
                let content = get(ns_window, b"contentView\0");
                if content.is_null() {
                    return;
                }
                call(content, b"retain\0");
                // Leave the window a placeholder so nothing that later asks it for its view gets nil.
                let placeholder = new_view(frame_of(content));
                set_ptr(ns_window, b"setContentView:\0", placeholder);
                call(placeholder, b"release\0");
                set_ptr(host.panel, b"setContentView:\0", content);
                call(content, b"release\0");
                host.adopted = Some((ns_window, content));
                log::info!("Fullscreen pill: moved the pill into the full-screen panel");
            }
            set_frame(host.panel, frame_of(ns_window));
            call(host.panel, b"orderFrontRegardless\0");
            set_ptr(ns_window, b"orderOut:\0", std::ptr::null_mut());
        });
    }

    /// Gives the panel the pill window's new size after a resize (e.g. for long errors), keeping
    /// the panel's top-left corner where it is since the user may have dragged it.
    pub fn sync_frame() {
        with_host(|host| unsafe {
            if let Some((ns_window, _)) = host.adopted {
                let panel = frame_of(host.panel);
                let size = frame_of(ns_window);
                let top = panel.y + panel.h;
                set_frame(host.panel, NSRect { x: panel.x, y: top - size.h, w: size.w, h: size.h });
            }
        });
    }

    /// Drags the panel if it's hosting the pill; returns false if the pill window should be dragged.
    pub fn start_dragging() -> bool {
        with_host(|host| unsafe {
            if host.adopted.is_none() {
                return false;
            }
            let app = get(objc_getClass(b"NSApplication\0".as_ptr()), b"sharedApplication\0");
            let event = get(app, b"currentEvent\0");
            if event.is_null() {
                return false;
            }
            set_ptr(host.panel, b"performWindowDragWithEvent:\0", event);
            true
        })
    }

    /// Hides the panel and puts the webview back into the pill window.
    pub fn release() {
        with_host(|host| unsafe {
            set_ptr(host.panel, b"orderOut:\0", std::ptr::null_mut());
            if let Some((ns_window, content)) = host.adopted.take() {
                set_frame(ns_window, frame_of(host.panel));
                call(content, b"retain\0");
                let placeholder = new_view(frame_of(content));
                set_ptr(host.panel, b"setContentView:\0", placeholder);
                call(placeholder, b"release\0");
                set_ptr(ns_window, b"setContentView:\0", content);
                call(content, b"release\0");
                log::info!("Fullscreen pill: moved the pill back into its window");
            }
        });
    }
}

/// Shows the pill in the full-screen panel instead of its own window. Call on the main thread.
pub(crate) fn adopt(win: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    if let Ok(ns_window) = win.ns_window() {
        mac::adopt(ns_window as *mut std::ffi::c_void);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = win;
}

/// Call on the main thread after resizing the pill window.
pub(crate) fn sync_frame() {
    #[cfg(target_os = "macos")]
    mac::sync_frame();
}

/// Drags the panel if it's hosting the pill; returns false if the pill window should be dragged.
pub(crate) fn start_dragging() -> bool {
    #[cfg(target_os = "macos")]
    return mac::start_dragging();
    #[cfg(not(target_os = "macos"))]
    false
}

/// Returns the pill to its own window (hidden). Call on the main thread.
pub(crate) fn release() {
    #[cfg(target_os = "macos")]
    mac::release();
}

/// Mirrors `assistant-state-changed`: once the pill goes idle it no longer needs the panel.
pub fn on_state_changed(app_handle: &AppHandle, state: &str) {
    if state == "idle" || state.is_empty() {
        let _ = app_handle.run_on_main_thread(release);
    }
}
