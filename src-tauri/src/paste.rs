use arboard::Clipboard;
use std::sync::{Mutex, OnceLock};
use std::thread::sleep;
use std::time::Duration;

#[cfg(not(target_os = "macos"))]
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

/// One clipboard for the whole run. On Linux the copied text is served by the app that set it,
/// so a `Clipboard` dropped right after `set_text` can take the text with it before Ctrl+V reads it.
static CLIPBOARD: OnceLock<Mutex<Clipboard>> = OnceLock::new();

fn clipboard() -> Result<&'static Mutex<Clipboard>, String> {
    if let Some(clipboard) = CLIPBOARD.get() {
        return Ok(clipboard);
    }
    let clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {}", e))?;
    Ok(CLIPBOARD.get_or_init(|| Mutex::new(clipboard)))
}

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    // Native Wayland apps read the Wayland clipboard; wl-copy writes it directly.
    #[cfg(target_os = "linux")]
    if crate::linux_paste::Session::detect().wayland && crate::linux_paste::on_path("wl-copy") {
        match crate::linux_paste::wl_copy(text) {
            Ok(()) => return Ok(()),
            Err(e) => log::warn!("wl-copy failed, using the X11 clipboard: {}", e),
        }
    }
    clipboard()?
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .set_text(text)
        .map_err(|e| format!("Failed to set clipboard text: {}", e))?;
    Ok(())
}

/// Copies `text` and sends the paste keys. `Ok(Some(note))` means the keys went out through a
/// fallback that may not reach every app; the note says what to do, and is given only once per
/// run. `Err` means the text is on the clipboard but couldn't be pasted.
pub fn copy_and_paste(text: &str) -> Result<Option<String>, String> {
    // Step 1: Copy to clipboard
    copy_to_clipboard(text)?;

    // Give OS a moment to register clipboard change
    sleep(Duration::from_millis(60));

    // Step 2: Simulate keyboard paste
    #[cfg(target_os = "macos")]
    {
        use std::ffi::c_void;
        type CGEventRef = *mut c_void;
        type CGEventSourceRef = *mut c_void;

        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGEventSourceCreate(stateID: i32) -> CGEventSourceRef;
            fn CGEventCreateKeyboardEvent(source: CGEventSourceRef, virtualKey: u16, keyDown: bool) -> CGEventRef;
            fn CGEventSetFlags(event: CGEventRef, flags: u64);
            fn CGEventPost(tap: u32, event: CGEventRef);
            fn CFRelease(cf: *const c_void);
        }

        const K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE: i32 = 0;
        const K_CG_SESSION_EVENT_TAP: u32 = 1;
        const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x00100000;
        const K_VK_ANSI_V: u16 = 0x09;

        // Without Accessibility macOS drops the keys silently. A new signing certificate also
        // voids the old grant, even though Settings still shows RevFly switched on.
        if !crate::global_key_listener::is_accessibility_trusted() {
            static PROMPT: std::sync::Once = std::sync::Once::new();
            PROMPT.call_once(crate::global_key_listener::request_accessibility_prompt);
            return Err("Turn on RevFly in Privacy & Security → Accessibility (remove it and add it again if it's already on). The text is on the clipboard.".to_string());
        }

        unsafe {
            let source = CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE);
            let down = CGEventCreateKeyboardEvent(source, K_VK_ANSI_V, true);
            let up = CGEventCreateKeyboardEvent(source, K_VK_ANSI_V, false);

            CGEventSetFlags(down, K_CG_EVENT_FLAG_MASK_COMMAND);
            CGEventSetFlags(up, 0);

            // Post single Cmd+V keyboard event to Session Event Tap
            CGEventPost(K_CG_SESSION_EVENT_TAP, down);
            CGEventPost(K_CG_SESSION_EVENT_TAP, up);

            CFRelease(down as _);
            CFRelease(up as _);
            if !source.is_null() {
                CFRelease(source as _);
            }
        }
        Ok(None)
    }

    #[cfg(target_os = "linux")]
    {
        use crate::linux_paste::{send_ctrl_v, PasteToolError, Session};
        let session = Session::detect();
        let tool_problem = match send_ctrl_v(session) {
            Ok(tool) => {
                log::info!("Pasted with {}", tool.command());
                return Ok(None);
            }
            Err(PasteToolError::NoTool) => None,
            Err(PasteToolError::Failed(why)) => Some(why),
        };
        enigo_ctrl_v().map_err(|e| {
            let why = tool_problem.as_deref().map(|t| format!("{t}; {e}")).unwrap_or(e);
            format!("The text is on the clipboard: press Ctrl+V. RevFly couldn't send the paste keys ({why}).")
        })?;
        // On Wayland, enigo's keys reach only XWayland apps.
        if session.wayland && take_wayland_note() {
            let note = match tool_problem {
                Some(_) => "If nothing was pasted, press Ctrl+V: the text is on the clipboard. Your paste tool failed; see the log in Settings › Logs.",
                None => "If nothing was pasted, press Ctrl+V: the text is on the clipboard. To paste into every app on Wayland, install ydotool, dotool or wtype.",
            };
            return Ok(Some(note.to_string()));
        }
        Ok(None)
    }

    #[cfg(target_os = "windows")]
    {
        enigo_ctrl_v().map_err(|e| format!("The text is on the clipboard: press Ctrl+V. RevFly couldn't send the paste keys ({e})."))?;
        Ok(None)
    }
}

#[cfg(not(target_os = "macos"))]
fn enigo_ctrl_v() -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    enigo.key(Key::Control, Direction::Press).map_err(|e| e.to_string())?;
    let click = enigo.key(Key::Unicode('v'), Direction::Click).map_err(|e| e.to_string());
    // Release Control even if the V failed, so it doesn't stay held.
    enigo.key(Key::Control, Direction::Release).map_err(|e| e.to_string())?;
    click
}

/// True the first time it's called in this run.
#[cfg(target_os = "linux")]
fn take_wayland_note() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static SHOWN: AtomicBool = AtomicBool::new(false);
    !SHOWN.swap(true, Ordering::SeqCst)
}
