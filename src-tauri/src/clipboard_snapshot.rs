//! A copy of everything on the clipboard (text, images, files, rich text, app-specific formats),
//! taken before a paste and put back after it for "Keep my clipboard".
//!
//! macOS and Windows keep every item in every format. Linux keeps the richest one: files, then an
//! image, then HTML, then text, because wl-copy and arboard serve only one format when they set it.

pub use platform::Snapshot;

#[cfg(target_os = "macos")]
mod platform {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSData, NSString};

    /// Every pasteboard item, each with every type it offers and that type's data.
    pub struct Snapshot(Vec<Vec<(String, Vec<u8>)>>);

    impl Snapshot {
        pub fn take() -> Option<Self> {
            let items = NSPasteboard::generalPasteboard().pasteboardItems()?;
            let items = items
                .iter()
                .map(|item| {
                    item.types()
                        .iter()
                        .filter_map(|kind| Some((kind.to_string(), item.dataForType(&kind)?.to_vec())))
                        .collect()
                })
                .collect();
            Some(Self(items))
        }

        pub fn restore(&self) -> Result<(), String> {
            let pasteboard = NSPasteboard::generalPasteboard();
            pasteboard.clearContents();
            if self.0.is_empty() {
                return Ok(());
            }
            let items: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = self
                .0
                .iter()
                .map(|kinds| {
                    let item = NSPasteboardItem::new();
                    for (kind, data) in kinds {
                        item.setData_forType(&NSData::with_bytes(data), &NSString::from_str(kind));
                    }
                    ProtocolObject::from_retained(item)
                })
                .collect();
            if pasteboard.writeObjects(&NSArray::from_retained_slice(&items)) {
                Ok(())
            } else {
                Err("the pasteboard refused the saved items".to_string())
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::time::Duration;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

    /// Every clipboard format whose data is memory, with that memory.
    pub struct Snapshot(Vec<(u32, Vec<u8>)>);

    /// The clipboard, open until dropped.
    struct Open;

    impl Open {
        /// Retries for a moment while another app has the clipboard open.
        fn new() -> Result<Self, String> {
            for _ in 0..20 {
                // SAFETY: opening needs no window; it is closed when `Open` drops.
                if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                    return Ok(Open);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err("another app is holding the clipboard".to_string())
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: the clipboard was opened by `Open::new`.
            unsafe { CloseClipboard() };
        }
    }

    /// False for formats whose data is a GDI handle or app-private (CF_BITMAP, CF_PALETTE,
    /// metafiles, display and private formats); Windows makes CF_BITMAP again from CF_DIB.
    fn is_memory(format: u32) -> bool {
        !matches!(format, 2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E | 0x200..=0x3FF)
    }

    impl Snapshot {
        pub fn take() -> Option<Self> {
            let _open = Open::new().ok()?;
            let mut formats = Vec::new();
            let mut format = 0;
            loop {
                // SAFETY: the clipboard is open; each handle is read only while it is locked.
                unsafe {
                    format = EnumClipboardFormats(format);
                    if format == 0 {
                        break;
                    }
                    if !is_memory(format) {
                        continue;
                    }
                    let handle = GetClipboardData(format);
                    if handle.is_null() {
                        continue;
                    }
                    let data = GlobalLock(handle) as *const u8;
                    if data.is_null() {
                        continue;
                    }
                    formats.push((format, std::slice::from_raw_parts(data, GlobalSize(handle)).to_vec()));
                    GlobalUnlock(handle);
                }
            }
            Some(Self(formats))
        }

        pub fn restore(&self) -> Result<(), String> {
            let _open = Open::new()?;
            // SAFETY: the clipboard is open; each block is filled while locked, and once
            // SetClipboardData takes it the clipboard owns it.
            unsafe {
                EmptyClipboard();
                for (format, bytes) in &self.0 {
                    let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
                    if handle.is_null() {
                        continue;
                    }
                    let data = GlobalLock(handle) as *mut u8;
                    if data.is_null() {
                        GlobalFree(handle);
                        continue;
                    }
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
                    GlobalUnlock(handle);
                    if SetClipboardData(*format, handle).is_null() {
                        GlobalFree(handle);
                    }
                }
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use crate::linux_paste::{on_path, Session};
    use std::path::PathBuf;

    /// The richest format on the clipboard.
    pub enum Snapshot {
        /// From the Wayland clipboard: a MIME type and its data.
        Wayland(Option<(String, Vec<u8>)>),
        Files(Vec<PathBuf>),
        Image(arboard::ImageData<'static>),
        Html { html: String, alt: Option<String> },
        Text(String),
        Empty,
    }

    fn wayland() -> bool {
        Session::detect().wayland && on_path("wl-paste") && on_path("wl-copy")
    }

    impl Snapshot {
        pub fn take() -> Option<Self> {
            if wayland() {
                match crate::linux_paste::wl_snapshot() {
                    Ok(best) => return Some(Self::Wayland(best)),
                    Err(e) => log::warn!("wl-paste failed, reading the X11 clipboard: {}", e),
                }
            }
            let mut clipboard = crate::paste::clipboard().ok()?.lock().unwrap_or_else(|e| e.into_inner());
            if let Ok(files) = clipboard.get().file_list() {
                if !files.is_empty() {
                    return Some(Self::Files(files));
                }
            }
            if let Ok(image) = clipboard.get_image() {
                return Some(Self::Image(image.to_owned_img()));
            }
            let text = clipboard.get_text().ok();
            if let Ok(html) = clipboard.get().html() {
                return Some(Self::Html { html, alt: text });
            }
            Some(text.map_or(Self::Empty, Self::Text))
        }

        pub fn restore(&self) -> Result<(), String> {
            if let Self::Wayland(best) = self {
                return crate::linux_paste::wl_restore(best.as_ref());
            }
            let mut clipboard = crate::paste::clipboard()?.lock().unwrap_or_else(|e| e.into_inner());
            match self {
                Self::Wayland(_) => Ok(()),
                Self::Files(files) => clipboard.set().file_list(files),
                Self::Image(image) => clipboard.set_image(image.clone()),
                Self::Html { html, alt } => clipboard.set_html(html.as_str(), alt.as_deref()),
                Self::Text(text) => clipboard.set_text(text),
                Self::Empty => clipboard.clear(),
            }
            .map_err(|e| e.to_string())
        }
    }
}
