//! Where the voice pill appears. It has one spot, kept as fractions of a monitor's work area (the
//! screen without the menu bar, Dock or taskbar), so it shows at the same place on whichever monitor
//! the cursor is on. Until the user drags it, the spot is centered, a little above the bottom. The
//! pill is always kept fully inside the work area, also when it grows for a long message.
//!
//! Positions are in points on macOS (one coordinate space across displays of any scale) and in
//! physical pixels elsewhere.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::Monitor;

/// Share of the work area's height left free below the pill until the user drags it.
const DEFAULT_BOTTOM_GAP: f64 = 0.10;

/// Set when the user starts dragging the pill; its spot is saved when it hides (see `take_dragged`).
static DRAGGED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Top-left corner for a `size` pill on `area`: its center at `center` (fractions of `area`) or the
/// default spot, moved as needed to be fully inside `area`.
pub fn place(area: Rect, size: (f64, f64), center: Option<(f64, f64)>) -> (f64, f64) {
    let (w, h) = size;
    let (cx, cy) = match center {
        Some((fx, fy)) => (area.x + fx * area.w, area.y + fy * area.h),
        None => (area.x + area.w / 2.0, area.y + area.h * (1.0 - DEFAULT_BOTTOM_GAP) - h / 2.0),
    };
    clamp_into(area, (cx - w / 2.0, cy - h / 2.0), size)
}

/// Moves a `size` frame at `pos` the least needed to be inside `area` (top-left wins if too big).
pub fn clamp_into(area: Rect, (x, y): (f64, f64), (w, h): (f64, f64)) -> (f64, f64) {
    (
        x.min(area.x + area.w - w).max(area.x),
        y.min(area.y + area.h - h).max(area.y),
    )
}

/// Where `frame`'s center is on `area`, as fractions of it.
pub fn center_in(area: Rect, frame: Rect) -> (f64, f64) {
    (
        ((frame.x + frame.w / 2.0 - area.x) / area.w).clamp(0.0, 1.0),
        ((frame.y + frame.h / 2.0 - area.y) / area.h).clamp(0.0, 1.0),
    )
}

pub fn mark_dragged() {
    DRAGGED.store(true, Ordering::SeqCst);
}

/// Whether the pill was dragged since this was last asked.
pub fn take_dragged() -> bool {
    DRAGGED.swap(false, Ordering::SeqCst)
}

fn work_area(monitor: &Monitor) -> Rect {
    let wa = monitor.work_area();
    let rect = Rect {
        x: wa.position.x as f64,
        y: wa.position.y as f64,
        w: wa.size.width as f64,
        h: wa.size.height as f64,
    };
    #[cfg(target_os = "macos")]
    {
        let s = monitor.scale_factor();
        Rect { x: rect.x / s, y: rect.y / s, w: rect.w / s, h: rect.h / s }
    }
    #[cfg(not(target_os = "macos"))]
    rect
}

/// A logical (CSS pixel) size in this module's units on `monitor`.
fn size_on(monitor: &Monitor, (w, h): (f64, f64)) -> (f64, f64) {
    #[cfg(target_os = "macos")]
    {
        let _ = monitor;
        (w, h)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let s = monitor.scale_factor();
        (w * s, h * s)
    }
}

fn set_top_left(win: &tauri::WebviewWindow, (x, y): (f64, f64)) {
    #[cfg(target_os = "macos")]
    let pos = tauri::Position::Logical(tauri::LogicalPosition { x, y });
    #[cfg(not(target_os = "macos"))]
    let pos = tauri::Position::Physical(tauri::PhysicalPosition { x: x.round() as i32, y: y.round() as i32 });
    let _ = win.set_position(pos);
}

/// The window's frame in this module's units.
fn frame_of(win: &tauri::WebviewWindow) -> Option<Rect> {
    let pos = win.outer_position().ok()?;
    let size = win.outer_size().ok()?;
    let rect = Rect { x: pos.x as f64, y: pos.y as f64, w: size.width as f64, h: size.height as f64 };
    #[cfg(target_os = "macos")]
    {
        let s = win.scale_factor().ok()?;
        Some(Rect { x: rect.x / s, y: rect.y / s, w: rect.w / s, h: rect.h / s })
    }
    #[cfg(not(target_os = "macos"))]
    Some(rect)
}

/// The monitor holding the center of `frame`, else the window's current one.
fn monitor_at(win: &tauri::WebviewWindow, frame: Rect) -> Option<Monitor> {
    let (cx, cy) = (frame.x + frame.w / 2.0, frame.y + frame.h / 2.0);
    let found = win.available_monitors().ok()?.into_iter().find(|m| {
        let a = work_area(m);
        cx >= a.x && cx < a.x + a.w && cy >= a.y && cy < a.y + a.h
    });
    found.or_else(|| win.current_monitor().ok().flatten())
}

/// Puts the pill window (`size` logical pixels) at its spot on `monitor`.
pub fn move_onto(win: &tauri::WebviewWindow, monitor: &Monitor, size: (f64, f64), center: Option<(f64, f64)>) {
    let pos = place(work_area(monitor), size_on(monitor, size), center);
    set_top_left(win, pos);
}

/// Before the pill window is resized to `size` (logical pixels): moves it so it keeps its center
/// and the new size still fits on its monitor.
pub fn reposition_for_size(win: &tauri::WebviewWindow, size: (f64, f64)) {
    let Some(frame) = frame_of(win) else { return };
    let Some(monitor) = monitor_at(win, frame) else { return };
    let (w, h) = size_on(&monitor, size);
    let (cx, cy) = (frame.x + frame.w / 2.0, frame.y + frame.h / 2.0);
    let pos = clamp_into(work_area(&monitor), (cx - w / 2.0, cy - h / 2.0), (w, h));
    set_top_left(win, pos);
}

/// The pill window's spot as fractions of the work area of the monitor it's on.
pub fn current_center(win: &tauri::WebviewWindow) -> Option<(f64, f64)> {
    let frame = frame_of(win)?;
    let monitor = monitor_at(win, frame)?;
    Some(center_in(work_area(&monitor), frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect { x: 0.0, y: 25.0, w: 1000.0, h: 800.0 };

    #[test]
    fn default_spot_is_centered_near_the_bottom() {
        let (x, y) = place(AREA, (340.0, 100.0), None);
        assert_eq!(x, 330.0);
        // The pill's bottom edge sits 10% of the height above the work area's bottom.
        assert_eq!(y + 100.0, AREA.y + AREA.h * 0.9);
    }

    #[test]
    fn saved_spot_is_relative_to_each_monitor() {
        let other = Rect { x: 1000.0, y: 0.0, w: 2000.0, h: 1000.0 };
        let center = center_in(AREA, Rect { x: 80.0, y: 75.0, w: 340.0, h: 100.0 });
        assert_eq!(center, (0.25, 0.125));
        assert_eq!(place(other, (340.0, 100.0), Some(center)), (1330.0, 75.0));
    }

    #[test]
    fn pill_never_leaves_the_work_area() {
        // Saved at the right edge, then grown for a long message.
        assert_eq!(place(AREA, (400.0, 360.0), Some((1.0, 1.0))), (600.0, 465.0));
        assert_eq!(clamp_into(AREA, (-50.0, -50.0), (340.0, 100.0)), (0.0, 25.0));
    }
}
