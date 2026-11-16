//! Where the notch panel's island sits in its window for each look. Pure
//! numbers over the measured notch, so the shapes are unit-tested.

use super::clock::Mode;

/// The panel's window: room for the open island, centred on the notch, its
/// top on the screen's top edge.
pub(crate) const PANEL_W: f32 = 420.0;
pub(crate) const PANEL_H: f32 = 240.0;
/// The window's middle, where the notch's middle goes.
pub(crate) const CX: f32 = PANEL_W / 2.0;

/// The open island's width.
pub(crate) const OPEN_W: f32 = 380.0;
/// Each ear's width beside the camera while a clock runs: room for
/// `0:00:00` at 13 pt.
pub(crate) const EAR_WIDTH: f32 = 56.0;
/// How far a hovered bare notch widens on each side, to say it opens.
const PEEK: f32 = 9.0;
/// How far a hovered bare notch drops below the menu bar.
const PEEK_DROP: f32 = 3.0;
/// The concave corner where a wall meets the screen's top edge.
pub(crate) const FILLET: f32 = 6.0;
pub(crate) const FILLET_KEYS: [&str; 2] = ["fillet-left", "fillet-right"];

/// The open island's insides, top to bottom.
pub(crate) const OPEN_PAD: f32 = 18.0;
pub(crate) const OPEN_TOP_GAP: f32 = 6.0;
pub(crate) const ROW_GAP: f32 = 8.0;
pub(crate) const TAB_H: f32 = 24.0;
pub(crate) const CLOCK_ROW: f32 = 50.0;
pub(crate) const PRESET_H: f32 = 24.0;
pub(crate) const TASK_H: f32 = 30.0;
const OPEN_BOTTOM: f32 = 16.0;
pub(crate) const ROUND_BUTTON: f32 = 38.0;

/// The notch the island grows from, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Notch {
    pub(crate) width: f32,
    /// The hardware notch's height (`safeAreaInsets.top`), or the menu
    /// bar's on a display without one.
    pub(crate) height: f32,
    /// The menu bar's height: the closed island's, so its bottom lines up
    /// with the bar's.
    pub(crate) bar: f32,
    /// Whether the notch is hardware; a pretend one is drawn only when
    /// something shows in it.
    pub(crate) real: bool,
}

impl Default for Notch {
    /// A 14-inch MacBook Pro's.
    fn default() -> Self {
        Self {
            width: 185.0,
            height: 32.0,
            bar: 34.0,
            real: true,
        }
    }
}

/// What the island shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    /// The notch alone.
    Bare,
    /// A hovered notch, a little wider, inviting a click.
    Peek,
    /// A clock's ears either side of the camera.
    Compact(Mode),
    /// Open, with the clocks and the task.
    Open(Mode),
}

/// The open island's height for `mode`.
pub(crate) fn open_height(mode: Mode, bar: f32) -> f32 {
    let presets = match mode {
        Mode::Timer => PRESET_H + ROW_GAP,
        Mode::Stopwatch => 0.0,
    };
    bar + OPEN_TOP_GAP + TAB_H + ROW_GAP + CLOCK_ROW + ROW_GAP + presets + TASK_H + OPEN_BOTTOM
}

/// The island's rect in the window, `[x, y, width, height]`, its top on the
/// screen's top edge.
pub(crate) fn island(look: Look, notch: Notch) -> [f32; 4] {
    let bar = notch.bar.max(notch.height);
    let (width, height) = match look {
        Look::Bare => (notch.width, notch.height),
        Look::Peek => (notch.width + 2.0 * PEEK, bar + PEEK_DROP),
        Look::Compact(_) => (notch.width + 2.0 * EAR_WIDTH, bar),
        Look::Open(mode) => (
            OPEN_W.max(notch.width + 2.0 * EAR_WIDTH),
            open_height(mode, bar),
        ),
    };
    let width = width.min(PANEL_W - 2.0 * FILLET);
    [CX - width / 2.0, 0.0, width, height.min(PANEL_H)]
}

/// The island's bottom corners.
pub(crate) fn radius(look: Look, notch: Notch) -> f32 {
    let [_, _, _, height] = island(look, notch);
    let radius: f32 = match look {
        Look::Bare => 8.0,
        Look::Peek => 10.0,
        Look::Compact(_) => 12.0,
        Look::Open(_) => 22.0,
    };
    radius.min(height / 2.0)
}

/// Where each ear's box starts, left and right of the camera.
pub(crate) fn ear_spans(notch: Notch) -> (f32, f32) {
    let half = notch.width / 2.0;
    (CX - half - EAR_WIDTH, CX + half)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_island_is_exactly_the_notch() {
        let notch = Notch::default();
        assert_eq!(island(Look::Bare, notch), [CX - 92.5, 0.0, 185.0, 32.0]);
    }

    #[test]
    fn ears_reach_the_menu_bars_bottom_beside_the_camera() {
        let notch = Notch::default();
        let [x, y, w, h] = island(Look::Compact(Mode::Timer), notch);
        assert_eq!((y, h), (0.0, 34.0), "the bar's height, not the camera's");
        assert_eq!(w, 185.0 + 2.0 * EAR_WIDTH);
        let (left, right) = ear_spans(notch);
        assert_eq!(left, x, "the left ear starts at the island's wall");
        assert_eq!(right + EAR_WIDTH, x + w, "the right ear ends at its wall");
    }

    #[test]
    fn the_open_island_fits_its_window_with_room_for_its_fillets() {
        for notch in [
            Notch::default(),
            Notch {
                width: 400.0,
                height: 40.0,
                bar: 40.0,
                real: true,
            },
        ] {
            for mode in [Mode::Stopwatch, Mode::Timer] {
                let [x, y, w, h] = island(Look::Open(mode), notch);
                assert!(x >= FILLET && x + w <= PANEL_W - FILLET);
                assert!(y + h <= PANEL_H);
            }
        }
        let timer = island(Look::Open(Mode::Timer), Notch::default())[3];
        let stopwatch = island(Look::Open(Mode::Stopwatch), Notch::default())[3];
        assert_eq!(
            timer - stopwatch,
            PRESET_H + ROW_GAP,
            "the timer adds its presets"
        );
    }

    #[test]
    fn a_hovered_notch_peeks_past_the_menu_bar() {
        let notch = Notch::default();
        let [_, _, w, h] = island(Look::Peek, notch);
        assert!(w > notch.width);
        assert!(h > notch.bar);
    }

    #[test]
    fn corners_never_exceed_half_the_height() {
        let tiny = Notch {
            width: 100.0,
            height: 10.0,
            bar: 10.0,
            real: false,
        };
        assert_eq!(radius(Look::Compact(Mode::Stopwatch), tiny), 5.0);
        assert_eq!(radius(Look::Open(Mode::Timer), Notch::default()), 22.0);
    }
}
