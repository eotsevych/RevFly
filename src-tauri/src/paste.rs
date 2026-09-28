use arboard::Clipboard;
use std::thread::sleep;
use std::time::Duration;

#[cfg(not(target_os = "macos"))]
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {}", e))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to set clipboard text: {}", e))?;
    Ok(())
}

pub fn copy_and_paste(text: &str) -> Result<(), String> {
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
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
            let _ = enigo.key(Key::Control, Direction::Press);
            let _ = enigo.key(Key::Unicode('v'), Direction::Click);
            let _ = enigo.key(Key::Control, Direction::Release);
        }
    }

    Ok(())
}
