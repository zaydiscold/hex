//! The notch panel on macOS: which screen has the notch, and handing the
//! keyboard back to the app in front when the panel closes.

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSRunningApplication, NSScreen, NSWorkspace,
};
use rune::OverlayHandle;

use super::Panel;
use super::geometry::{CX, Notch, PANEL_H, PANEL_W};

/// The notch to grow from and the window's rect in Rune's global points (from
/// the primary screen's top-left, y down): the first screen with a hardware
/// notch, else the main screen with a pretend one the menu bar's height.
pub(super) fn measure() -> Option<(Notch, [f32; 4])> {
    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    let primary_top = {
        let primary = screens.iter().next()?;
        primary.frame().origin.y + primary.frame().size.height
    };
    let notched = screens
        .iter()
        .map(|screen| crate::rune_hud::screen_metrics(&screen))
        .find(|metrics| metrics.notch().is_some());
    let (metrics, real) = match notched {
        Some(metrics) => (metrics, true),
        None => (
            crate::rune_hud::screen_metrics(&*NSScreen::mainScreen(mtm)?).pretending_notch(),
            false,
        ),
    };
    let notch = metrics.notch()?;
    let top = metrics.frame.y + metrics.frame.height;
    let measured = Notch {
        width: notch.width as f32,
        height: notch.height as f32,
        bar: metrics.bar_height().max(notch.height) as f32,
        real,
    };
    let rect = [
        (notch.x - f64::from(CX)) as f32,
        (primary_top - top) as f32,
        PANEL_W,
        PANEL_H,
    ];
    Some((measured, rect))
}

/// The app in front, which keeps the keyboard until the panel asks for it.
pub(super) fn front_pid() -> Option<i32> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app.processIdentifier();
    (pid != std::process::id() as i32).then_some(pid)
}

/// Gives the keyboard back: the panel stops being the key window (ordered
/// out and straight back in, which AppKit never makes key again), and the
/// app that was in front when it opened is in front again.
pub(super) fn give_keys_back(overlay: &OverlayHandle<Panel>, front: Option<i32>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(number) = overlay.window_number() else {
        return;
    };
    let panel = NSApplication::sharedApplication(mtm)
        .windows()
        .iter()
        .find(|window| window.windowNumber() == number as isize);
    let Some(panel) = panel else {
        return;
    };
    if !panel.isKeyWindow() {
        return;
    }
    panel.orderOut(None);
    panel.orderFrontRegardless();
    if let Some(app) = front.and_then(NSRunningApplication::runningApplicationWithProcessIdentifier)
    {
        app.activateWithOptions(NSApplicationActivationOptions::empty());
    }
}
