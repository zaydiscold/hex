//! How far the dictation HUD sits from its screen edge, in its own heights:
//! down from the top, up from the bottom (macOS).

use std::sync::atomic::{AtomicU8, Ordering};

/// The furthest the HUD moves from its edge.
pub const MAX: u8 = 4;

static OFFSET: AtomicU8 = AtomicU8::new(0);

/// Publishes the saved offset, at most [`MAX`].
pub(super) fn apply(heights: u8) {
    OFFSET.store(heights.min(MAX), Ordering::Relaxed);
}

/// The offset the HUD is placed with, 0 to [`MAX`].
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn get() -> u8 {
    OFFSET.load(Ordering::Relaxed)
}
