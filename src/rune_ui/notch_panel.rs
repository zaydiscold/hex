//! Labs › Notch panel (macOS): the notch as a small focus tool. Click it and
//! a black island grows out of it with a stopwatch, a countdown timer, and
//! the task at hand; click it again, press Esc, or move away and it shrinks
//! back. While a clock runs, the notch keeps two ears out: the clock's glyph
//! on the left, its time on the right, and a countdown that ends turns them
//! green and rings once.
//!
//! The panel is its own interactive overlay over the notch, above the menu
//! bar. Only the island takes the pointer (`OverlayInput::Regions`): the rest
//! of its window lets clicks through to the menu bar beneath. It asks for
//! the keyboard only while open, for the task field and Esc, and gives it
//! back to the app in front when it closes. While the dictation HUD shows,
//! a closed panel tucks its ears in, so the HUD's own ears have the notch.
//!
//! The clocks count wall-clock time (`clock`), saved to `notch-panel.json`
//! in Hex's Application Support folder after every change, so a running
//! clock keeps counting across a restart.

pub(crate) mod clock;
mod geometry;

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use rune::{
    Color, Cursor, Cx, El, Key, KeyEvent, OverlayHandle, Shape, Spring, SpringPair, TextEdit,
    TextStyle, Transition, Tx, canvas, col, edit_access, el, row, text,
};
use rune_kit::TextInput;

use self::clock::{Clock, Mode, Tick};
use self::geometry::{Look, Notch, PANEL_H, PANEL_W};
use super::controls::Paint;

/// The island's walls as it grows and shrinks: quick, with a little give.
const SHELL: Spring = Spring::new(0.42, 0.18);
/// Content fading in once the walls open, and out as they close.
const CONTENT_IN: Spring = Spring::new(0.30, 0.0);
const CONTENT_OUT: Spring = Spring::new(0.14, 0.0);
/// How long an open panel waits after the pointer leaves before closing.
const LEAVE_GRACE: Duration = Duration::from_millis(450);
/// How often a closed panel checks which screen has the notch.
const REMEASURE: Duration = Duration::from_secs(4);
/// Above the menu bar and its status items, as the HUD's island sits.
#[cfg(target_os = "macos")]
const LEVEL: isize = 26;

/// The countdown presets, in minutes.
const PRESETS: [i64; 5] = [5, 15, 25, 45, 60];

static PANEL: OnceLock<OverlayHandle<Panel>> = OnceLock::new();

/// The notch panel's state.
pub(crate) struct Panel {
    clock: Clock,
    task: TextEdit,
    /// Labs › Notch panel is on.
    enabled: bool,
    open: bool,
    /// The pointer is over the island.
    hovered: bool,
    /// When the pointer left an open island.
    left_at: Option<Instant>,
    /// The dictation HUD is showing.
    dictating: bool,
    notch: Notch,
    /// The window rect last placed, in Rune's global points.
    placed: Option<[f32; 4]>,
    measured_at: Option<Instant>,
    /// The second last drawn, so the poll asks for a frame only when the
    /// shown time changes.
    drawn_second: Option<i64>,
    this: Option<OverlayHandle<Panel>>,
    /// The app in front when the panel opened, to give the keyboard back to.
    #[cfg(target_os = "macos")]
    front_pid: Option<i32>,
    /// Where the clock is saved; `None` keeps it in memory (tests).
    path: Option<PathBuf>,
}

impl Panel {
    fn new(path: Option<PathBuf>) -> Self {
        let clock = path.as_deref().map(load).unwrap_or_default();
        Self {
            task: TextEdit::new(clock.task.clone()),
            clock,
            enabled: false,
            open: false,
            hovered: false,
            left_at: None,
            dictating: false,
            notch: Notch::default(),
            placed: None,
            measured_at: None,
            drawn_second: None,
            this: None,
            #[cfg(target_os = "macos")]
            front_pid: None,
            path,
        }
    }

    /// What the island looks like now.
    fn look(&self) -> Look {
        if self.open {
            return Look::Open(self.clock.mode);
        }
        if self.dictating {
            return Look::Bare;
        }
        match self.clock.compact() {
            Some(mode) => Look::Compact(mode),
            None if self.hovered => Look::Peek,
            None => Look::Bare,
        }
    }

    fn now() -> i64 {
        crate::calendar::wall_clock_ms()
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        match serde_json::to_vec_pretty(&self.clock) {
            Ok(bytes) => {
                if let Err(error) = crate::durable::write(path, &bytes) {
                    tracing::warn!(%error, "could not save the notch panel");
                }
            }
            Err(error) => tracing::warn!(%error, "could not encode the notch panel"),
        }
    }

    fn expand(&mut self, tx: &mut Tx) {
        if self.open {
            return;
        }
        // A ring is answered by looking at it.
        if self.clock.rang {
            self.clock.acknowledge();
            self.save();
        }
        self.open = true;
        self.left_at = None;
        self.measure();
        #[cfg(target_os = "macos")]
        {
            self.front_pid = macos::front_pid();
            if let Some(this) = &self.this {
                this.request_keys();
            }
        }
        tx.animated();
    }

    fn collapse(&mut self, tx: &mut Tx) {
        if !self.open {
            return;
        }
        self.commit_task();
        self.task.focused = false;
        self.open = false;
        self.left_at = None;
        #[cfg(target_os = "macos")]
        if let Some(this) = &self.this {
            macos::give_keys_back(this, self.front_pid.take());
        }
        tx.animated();
    }

    fn hover(&mut self, inside: bool, tx: &mut Tx) {
        self.hovered = inside;
        if inside {
            self.left_at = None;
        } else if self.open && !self.task.focused {
            self.left_at = Some(Instant::now());
            tx.wake_after(LEAVE_GRACE);
        }
        tx.animated();
    }

    fn commit_task(&mut self) {
        let before = self.clock.task.clone();
        self.clock.set_task(&self.task.text);
        if self.clock.task != before {
            self.save();
        }
    }

    fn set_mode(&mut self, mode: Mode, tx: &mut Tx) {
        if self.clock.mode != mode {
            self.clock.mode = mode;
            self.save();
            tx.animated();
        }
    }

    fn toggle_clock(&mut self, tx: &mut Tx) {
        self.clock.toggle(Self::now());
        self.save();
        self.drawn_second = None;
        tx.animated();
    }

    fn reset_clock(&mut self, tx: &mut Tx) {
        self.clock.reset();
        self.save();
        tx.animated();
    }

    fn preset(&mut self, minutes: i64, tx: &mut Tx) {
        self.clock.set_timer(minutes * 60_000, Self::now());
        self.save();
        tx.animated();
    }

    fn nudge(&mut self, minutes: i64, tx: &mut Tx) {
        self.clock.nudge_timer(minutes * 60_000, Self::now());
        self.save();
        tx.animated();
    }

    fn toggle_done(&mut self, tx: &mut Tx) {
        self.commit_task();
        if !self.clock.task.is_empty() {
            self.clock.task_done = !self.clock.task_done;
            self.save();
            tx.animated();
        }
    }

    /// Labs turned the panel on or off.
    fn set_enabled(&mut self, on: bool, tx: &mut Tx) {
        if self.enabled == on {
            return;
        }
        self.enabled = on;
        let Some(this) = self.this.clone() else {
            return;
        };
        if on {
            self.placed = None;
            self.measure();
            this.show();
        } else {
            self.collapse(tx);
            this.hide();
        }
    }

    /// Finds the notch and moves the window onto it when it moved.
    fn measure(&mut self) {
        self.measured_at = Some(Instant::now());
        #[cfg(target_os = "macos")]
        if let Some((notch, rect)) = macos::measure() {
            self.notch = notch;
            if self.placed != Some(rect)
                && let Some(this) = &self.this
            {
                this.set_display(rune::DisplayTarget::Primary);
                this.set_placement(Some(rune::OverlayPlacement::Rect {
                    x: rect[0],
                    y: rect[1],
                    width: rect[2],
                    height: rect[3],
                }));
                self.placed = Some(rect);
            }
        }
    }
}

/// The saved clock at `path`, or a fresh one.
fn load(path: &std::path::Path) -> Clock {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Clock>(&bytes).ok())
        .map(Clock::sanitized)
        .unwrap_or_default()
}

/// Before each frame: rings a countdown that ended, closes an open panel
/// the pointer left, and wakes for the next second a running clock shows.
fn poll(p: &mut Panel, _: f64, tx: &mut Tx) -> bool {
    let now = Panel::now();
    let mut changed = false;
    if p.clock.tick(now) == Tick::Rang {
        p.save();
        ring();
        changed = true;
        tx.animated();
    }
    if p.open
        && !p.hovered
        && !p.task.focused
        && p.left_at.is_some_and(|at| at.elapsed() >= LEAVE_GRACE)
    {
        p.collapse(tx);
        changed = true;
    }
    if !p.open && p.measured_at.is_none_or(|at| at.elapsed() >= REMEASURE) {
        p.measure();
    }
    let shown = p.clock.compact().or(p.open.then_some(p.clock.mode));
    let second = shown.map(|mode| p.clock.shown_ms(mode, now) / 1000);
    if second != p.drawn_second {
        p.drawn_second = second;
        changed = true;
    }
    let mut wake = REMEASURE;
    if let Some(ms) = p.clock.next_change_ms(now) {
        wake = wake.min(Duration::from_millis(ms as u64));
    }
    if let Some(at) = p.left_at {
        wake = wake.min(LEAVE_GRACE.saturating_sub(at.elapsed()));
    }
    tx.wake_after(wake);
    changed
}

fn on_key(p: &mut Panel, key: &KeyEvent, tx: &mut Tx) -> bool {
    if !p.open {
        return false;
    }
    match &key.key {
        Key::Escape => {
            p.collapse(tx);
            true
        }
        Key::Text(t) if t == " " && !p.task.focused && !key.mods.cmd => {
            p.toggle_clock(tx);
            true
        }
        _ => false,
    }
}

/// Another app took the keyboard back: an open panel closes.
fn on_focus(p: &mut Panel, focused: bool, tx: &mut Tx) {
    if !focused && p.open {
        p.commit_task();
        p.task.focused = false;
        p.open = false;
        p.left_at = None;
        #[cfg(target_os = "macos")]
        {
            p.front_pid = None;
        }
        tx.animated();
    }
}

/// A countdown ended: the system's Glass sound, once.
fn ring() {
    #[cfg(target_os = "macos")]
    {
        let spawned = std::process::Command::new("/usr/bin/afplay")
            .args(["-v", "0.8", "/System/Library/Sounds/Glass.aiff"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match spawned {
            // Reaped off the main thread so it never lingers as a zombie.
            Ok(mut child) => {
                let _ = std::thread::Builder::new()
                    .name("hex-notch-ring".into())
                    .spawn(move || child.wait());
            }
            Err(error) => tracing::warn!(%error, "could not ring the notch timer"),
        }
    }
}

/// Adds the panel's overlay, hidden until Labs turns it on.
#[cfg(target_os = "macos")]
pub(crate) fn install<S: 'static>(app: &mut rune::App<S>) {
    use rune::{KeyPolicy, OverlayInput, OverlayOptions, OverlaySpec};
    let path = crate::app_paths::support_dir()
        .ok()
        .map(|dir| dir.join("notch-panel.json"));
    let spec = OverlaySpec::new("Hex notch panel", (PANEL_W, PANEL_H), view)
        .options(OverlayOptions {
            input: OverlayInput::Regions,
            keys: KeyPolicy::OnDemand,
            ..OverlayOptions::default()
        })
        .macos_level(LEVEL)
        .on_key(on_key)
        .on_focus(on_focus)
        .poll(poll);
    let overlay = app.overlay(spec, Panel::new(path));
    let this = overlay.clone();
    overlay.update(move |p, _| p.this = Some(this));
    let _ = PANEL.set(overlay);
    reconcile(crate::labs::notch_panel());
}

/// Shows the panel while Labs › Notch panel is on and hides it while off.
/// Safe from any thread; a no-op before the panel is installed.
pub(crate) fn reconcile(on: bool) {
    if let Some(panel) = PANEL.get() {
        panel.update(move |p, tx| p.set_enabled(on, tx));
    }
}

/// The dictation HUD showed or hid: a closed panel tucks its ears in while
/// the HUD's are out.
pub(crate) fn hud_shown(shown: bool) {
    if let Some(panel) = PANEL.get() {
        panel.update(move |p, tx| {
            if p.dictating != shown {
                p.dictating = shown;
                tx.animated();
            }
        });
    }
}

// ---------------------------------------------------------------------------
// View

/// White on the island, in either appearance.
const INK: Color = Color::WHITE;

fn ink(alpha: f32) -> Color {
    INK.alpha(alpha)
}

fn green() -> Color {
    rune::oklch(0.80, 0.17, 150.0)
}

fn amber() -> Color {
    rune::oklch(0.80, 0.15, 70.0)
}

/// The clock's colour: green for the stopwatch, amber for the countdown,
/// green again once it ends.
fn tint(mode: Mode, rang: bool) -> Color {
    match mode {
        Mode::Timer if !rang => amber(),
        _ => green(),
    }
}

fn mono(style: TextStyle, size: f32, weight: f32) -> TextStyle {
    TextStyle {
        size,
        weight,
        line_height: 0.0,
        ..style
    }
    .features("'tnum' 1")
}

fn view(p: &Panel, cx: &mut Cx<Panel>) -> El {
    // The island is black in either appearance, so what's on it is dark's.
    let paint = Paint::new(true, cx.env().scale);
    let look = p.look();
    let now = Panel::now();
    let [x, y, w, h] = geometry::island(look, p.notch);
    let radius = geometry::radius(look, p.notch);
    let fill = if look == Look::Bare && !p.notch.real {
        // A display without a notch shows nothing until hovered.
        Color::TRANSPARENT
    } else {
        Color::BLACK
    };
    let shell = el()
        .key("shell")
        .absolute(x, y)
        .size(w, h)
        .corners([0.0, 0.0, radius, radius])
        .fill(fill)
        .motion(SHELL);
    // The fillets where the island's walls meet the screen's top edge,
    // out whenever the island is wider than the notch.
    let fillets = look != Look::Bare;
    let [left, right] = geometry::FILLET_KEYS.map(|key| {
        let right = key.ends_with("right");
        let fx = if right { x + w } else { x - geometry::FILLET };
        fillet(right)
            .key(key)
            .absolute(fx, 0.0)
            .opacity(if fillets { 1.0 } else { 0.0 })
            .motion(SHELL)
    });
    // Content stays put while the walls move over it: a box on the shell's
    // rect clips, and the box inside moves back by the same amount.
    let mut content = el()
        .key("content")
        .absolute(-x, -y)
        .size(PANEL_W, PANEL_H)
        .motion(SHELL);
    let presence = Transition::opacity().springs(SpringPair::new(CONTENT_IN, CONTENT_OUT));
    match look {
        Look::Open(mode) => {
            content = content.child(open_view(p, paint, mode, now).transition(presence));
        }
        Look::Compact(mode) => {
            content = content.child(ears(p, paint, mode, now).transition(presence));
        }
        Look::Bare | Look::Peek => {}
    }
    let label = match look {
        Look::Open(_) => "Close the notch panel",
        _ => "Open the notch panel",
    };
    let clip = el()
        .key("island")
        .absolute(x, y)
        .size(w, h)
        .motion(SHELL)
        .clip_rounded(radius)
        .role("button")
        .label(label)
        .cursor(if p.open {
            Cursor::Default
        } else {
            Cursor::Pointer
        })
        .on_hover_change(|p: &mut Panel, inside, tx| p.hover(inside, tx))
        .on_press_tx(|p: &mut Panel, tx| {
            if !p.open {
                p.expand(tx);
            }
        })
        .hit()
        .child(content);
    el().key("panel")
        .size(PANEL_W, PANEL_H)
        .child(left)
        .child(right)
        .child(shell)
        .child(clip)
}

/// One fillet: the concave corner between a wall and the screen's top edge.
fn fillet(right: bool) -> El {
    const K: f32 = 0.552;
    let r = geometry::FILLET;
    let path = if right {
        rune::Path::new()
            .move_to([r, 0.0])
            .line_to([0.0, 0.0])
            .line_to([0.0, r])
            .cubic_to([0.0, r - K * r], [r - K * r, 0.0], [r, 0.0])
    } else {
        rune::Path::new()
            .move_to([0.0, 0.0])
            .line_to([r, 0.0])
            .line_to([r, r])
            .cubic_to([r, r - K * r], [K * r, 0.0], [0.0, 0.0])
    }
    .close();
    canvas(Shape::new().view_box(r, r).fill_path(&path, Color::BLACK))
        .size(r, r)
        .a11y_hidden()
}

/// The closed island's ears: the clock's glyph left of the camera, its
/// time right of it.
fn ears(p: &Panel, paint: Paint, mode: Mode, now: i64) -> El {
    let notch = p.notch;
    let (left_x, right_x) = geometry::ear_spans(notch);
    let bar = notch.bar;
    let rang = mode == Mode::Timer && p.clock.rang;
    let color = tint(mode, rang);
    let glyph = match mode {
        Mode::Timer if rang => check_glyph(color, 14.0),
        Mode::Timer => ring_glyph(p.clock.timer_progress(now), color, 14.0),
        Mode::Stopwatch => stopwatch_glyph(color, 14.0),
    };
    let time = if rang {
        "Done".to_owned()
    } else {
        clock::format(p.clock.shown_ms(mode, now), mode == Mode::Timer)
    };
    let mut time_text = text(time.clone())
        .key("compact-time")
        .font(mono(paint.k.ty.ui_medium, 13.0, 600.0))
        .color(if rang { color } else { INK })
        .single_line();
    if rang {
        time_text = time_text.looping(rune::LoopSpec::opacity(1.2, 0.0, 1.0, 0.35));
    }
    let glyph_box = row()
        .key("compact-glyph")
        .absolute(left_x, 0.0)
        .size(geometry::EAR_WIDTH, bar)
        .center()
        .child(glyph);
    let time_box = row()
        .key("compact-time-box")
        .absolute(right_x, 0.0)
        .size(geometry::EAR_WIDTH, bar)
        .center()
        .child(time_text);
    el().key("ears")
        .size(PANEL_W, PANEL_H)
        .role("status")
        .label(format!(
            "{}: {time}",
            match mode {
                Mode::Stopwatch => "Stopwatch",
                Mode::Timer => "Timer",
            }
        ))
        .child(glyph_box)
        .child(time_box)
}

fn open_view(p: &Panel, paint: Paint, mode: Mode, now: i64) -> El {
    let [x, _, w, _] = geometry::island(Look::Open(mode), p.notch);
    let running = p.clock.running();
    let rang = mode == Mode::Timer && p.clock.rang;
    let color = tint(mode, rang);
    let ms = p.clock.shown_ms(mode, now);
    let time = clock::format(ms, mode == Mode::Timer);
    let mut body = col()
        .key("open")
        .absolute(x, p.notch.bar)
        .w(w)
        .px(geometry::OPEN_PAD)
        .pt(geometry::OPEN_TOP_GAP)
        .gap(geometry::ROW_GAP)
        .child(tabs(paint, mode))
        .child(
            row()
                .key("clock-row")
                .h(geometry::CLOCK_ROW)
                .center_y()
                .gap(10.0)
                .child(
                    text(time)
                        .key("time")
                        .font(mono(paint.k.ty.ui_medium, 40.0, 600.0))
                        .color(if running || rang { color } else { INK })
                        .single_line()
                        .grow(1.0)
                        .role("timer"),
                )
                .child(round_button(
                    "reset",
                    if mode == Mode::Timer {
                        "Reset timer"
                    } else {
                        "Reset stopwatch"
                    },
                    reset_glyph(ink(0.85), 16.0),
                    ink(0.14),
                    p.clock.resettable(now),
                    |p, tx| p.reset_clock(tx),
                ))
                .child(round_button(
                    "start",
                    if running { "Pause" } else { "Start" },
                    if running {
                        pause_glyph(Color::BLACK, 16.0)
                    } else {
                        play_glyph(Color::BLACK, 16.0)
                    },
                    if running { INK } else { color },
                    true,
                    |p, tx| p.toggle_clock(tx),
                )),
        );
    if mode == Mode::Timer {
        body = body.child(presets(paint, p));
    }
    body.child(task_row(p, paint))
}

/// Stopwatch | Timer.
fn tabs(paint: Paint, mode: Mode) -> El {
    let tab = |key: &'static str, label: &'static str, this: Mode| {
        let on = mode == this;
        row()
            .key(key)
            .h(geometry::TAB_H)
            .px(12.0)
            .center()
            .radius(geometry::TAB_H / 2.0)
            .fill(if on { ink(0.16) } else { Color::TRANSPARENT })
            .hover(ink(0.10))
            .cursor(Cursor::Pointer)
            .role("tab")
            .label(label)
            .on_press_tx(move |p: &mut Panel, tx| p.set_mode(this, tx))
            .hit()
            .child(
                text(label)
                    .font(paint.k.ty.small)
                    .color(if on { INK } else { ink(0.55) })
                    .a11y_hidden(),
            )
    };
    row()
        .key("tabs")
        .role("tablist")
        .gap(4.0)
        .child(tab("tab-stopwatch", "Stopwatch", Mode::Stopwatch))
        .child(tab("tab-timer", "Timer", Mode::Timer))
}

/// The countdown's lengths, and a minute less or more.
fn presets(paint: Paint, p: &Panel) -> El {
    let length = p.clock.timer_ms;
    let mut chips = row()
        .key("presets")
        .h(geometry::PRESET_H)
        .gap(6.0)
        .center_y();
    for minutes in PRESETS {
        let on = length == minutes * 60_000;
        chips = chips.child(
            row()
                .key(format!("preset-{minutes}"))
                .h(geometry::PRESET_H)
                .px(9.0)
                .center()
                .radius(geometry::PRESET_H / 2.0)
                .fill(if on { ink(0.9) } else { ink(0.10) })
                .hover(if on { ink(0.95) } else { ink(0.16) })
                .cursor(Cursor::Pointer)
                .role("button")
                .label(format!("{minutes} minutes"))
                .on_press_tx(move |p: &mut Panel, tx| p.preset(minutes, tx))
                .hit()
                .child(
                    text(clock::format_length(minutes * 60_000))
                        .font(mono(paint.k.ty.small, 12.0, 600.0))
                        .color(if on { Color::BLACK } else { ink(0.8) })
                        .a11y_hidden(),
                ),
        );
    }
    let left = clock::format_length(length);
    chips
        .child(el().grow(1.0))
        .child(nudge_button("minus", "−", "One minute less", -1))
        .child(
            text(left)
                .key("length")
                .font(mono(paint.k.ty.small, 12.0, 500.0))
                .color(ink(0.55))
                .single_line(),
        )
        .child(nudge_button("plus", "+", "One minute more", 1))
}

fn nudge_button(key: &'static str, glyph: &'static str, label: &'static str, minutes: i64) -> El {
    row()
        .key(key)
        .size(geometry::PRESET_H, geometry::PRESET_H)
        .center()
        .radius(geometry::PRESET_H / 2.0)
        .fill(ink(0.10))
        .hover(ink(0.18))
        .cursor(Cursor::Pointer)
        .role("button")
        .label(label)
        .on_press_tx(move |p: &mut Panel, tx| p.nudge(minutes, tx))
        .hit()
        .child(
            text(glyph)
                .font(TextStyle::new(15.0, 600.0))
                .color(ink(0.85))
                .a11y_hidden(),
        )
}

/// The task: a check, and the field.
fn task_row(p: &Panel, paint: Paint) -> El {
    let done = p.clock.task_done && !p.clock.task.is_empty();
    let check = row()
        .key("task-check")
        .size(18.0, 18.0)
        .center()
        .radius(9.0)
        .fill(if done { green() } else { Color::TRANSPARENT })
        .border(1.5, if done { green() } else { ink(0.4) })
        .cursor(Cursor::Pointer)
        .role("checkbox")
        .label("Task done")
        .checked(done)
        .on_press_tx(|p: &mut Panel, tx| p.toggle_done(tx))
        .hit()
        .children(done.then(|| check_glyph(Color::BLACK, 10.0)));
    let field = TextInput::new(&p.task)
        .placeholder("What are you working on?")
        .plain()
        .width(None)
        .height(geometry::TASK_H)
        .label("Task")
        .on_edit(edit_access(|p: &mut Panel| &mut p.task))
        .on_change(|p: &mut Panel, _| p.left_at = None)
        .on_submit(|p: &mut Panel, tx| {
            p.commit_task();
            p.collapse(tx);
        })
        .build(paint.k)
        .key("task")
        .grow(1.0)
        .min_w(0.0)
        .opacity(if done { 0.5 } else { 1.0 })
        .hit();
    row()
        .key("task-row")
        .h(geometry::TASK_H)
        .px(10.0)
        .gap(8.0)
        .center_y()
        .radius(geometry::TASK_H / 2.0)
        .fill(ink(0.08))
        .child(check)
        .child(field)
}

fn round_button(
    key: &'static str,
    label: &'static str,
    glyph: El,
    fill: Color,
    enabled: bool,
    press: impl Fn(&mut Panel, &mut Tx) + 'static,
) -> El {
    let size = geometry::ROUND_BUTTON;
    let button = row()
        .key(key)
        .size(size, size)
        .center()
        .radius(size / 2.0)
        .fill(fill)
        .role("button")
        .label(label)
        .child(glyph.a11y_hidden());
    if !enabled {
        return button.opacity(0.35).disabled(true);
    }
    button
        .hover(ink(0.12))
        .press_scale(0.94)
        .cursor(Cursor::Pointer)
        .on_press_tx(press)
        .hit()
}

// ---------------------------------------------------------------------------
// Glyphs, drawn so they match on every Mac

fn circle_points(cx: f32, cy: f32, r: f32, from: f32, to: f32, steps: usize) -> Vec<[f32; 2]> {
    (0..=steps)
        .map(|i| {
            let t = from + (to - from) * i as f32 / steps as f32;
            [cx + r * t.sin(), cy - r * t.cos()]
        })
        .collect()
}

/// A countdown's progress: a faint ring with the part left drawn bright.
fn ring_glyph(progress: f32, color: Color, size: f32) -> El {
    let c = size / 2.0;
    let r = c - 1.5;
    let full = std::f32::consts::TAU;
    let left = (1.0 - progress).clamp(0.0, 1.0);
    let mut shape = Shape::new().view_box(size, size).stroke(
        circle_points(c, c, r, 0.0, full, 48),
        2.0,
        color.alpha(0.25),
    );
    if left > 0.0 {
        shape = shape.stroke(
            circle_points(c, c, r, full * (1.0 - left), full, 48),
            2.0,
            color,
        );
    }
    canvas(shape).size(size, size).key("ring")
}

/// A stopwatch: a dial, a crown, and a hand.
fn stopwatch_glyph(color: Color, size: f32) -> El {
    let c = size / 2.0;
    let r = c - 2.0;
    let cy = c + 1.0;
    let shape = Shape::new()
        .view_box(size, size)
        .stroke(
            circle_points(c, cy, r, 0.0, std::f32::consts::TAU, 40),
            1.6,
            color,
        )
        .line([c, 0.5], [c, cy - r], 1.8, rune::LineCap::Round, color)
        .line(
            [c, cy],
            [c + r * 0.45, cy - r * 0.55],
            1.6,
            rune::LineCap::Round,
            color,
        );
    canvas(shape).size(size, size).key("stopwatch")
}

fn check_glyph(color: Color, size: f32) -> El {
    let shape = Shape::new().view_box(size, size).stroke(
        vec![
            [size * 0.18, size * 0.52],
            [size * 0.42, size * 0.76],
            [size * 0.84, size * 0.26],
        ],
        size * 0.16,
        color,
    );
    canvas(shape).size(size, size).key("check")
}

fn play_glyph(color: Color, size: f32) -> El {
    let shape = Shape::new().view_box(size, size).fill(
        vec![
            [size * 0.28, size * 0.16],
            [size * 0.86, size * 0.5],
            [size * 0.28, size * 0.84],
        ],
        color,
    );
    canvas(shape).size(size, size).key("play")
}

fn pause_glyph(color: Color, size: f32) -> El {
    let bar = |x: f32| {
        vec![
            [x, size * 0.18],
            [x + size * 0.22, size * 0.18],
            [x + size * 0.22, size * 0.82],
            [x, size * 0.82],
        ]
    };
    let shape = Shape::new()
        .view_box(size, size)
        .fill(bar(size * 0.2), color)
        .fill(bar(size * 0.58), color);
    canvas(shape).size(size, size).key("pause")
}

/// A circular arrow: start over.
fn reset_glyph(color: Color, size: f32) -> El {
    let c = size / 2.0;
    let r = c - 2.0;
    let start = -0.9;
    let end = std::f32::consts::TAU - 1.4;
    let arc = circle_points(c, c, r, start, end, 36);
    let tip = *arc.last().expect("an arc has points");
    let head = vec![
        [tip[0] - 3.6, tip[1] - 1.6],
        tip,
        [tip[0] - 0.4, tip[1] + 3.8],
    ];
    let shape = Shape::new()
        .view_box(size, size)
        .stroke(arc, 1.7, color)
        .stroke(head, 1.7, color);
    canvas(shape).size(size, size).key("reset-glyph")
}

#[cfg(target_os = "macos")]
mod macos;

#[cfg(test)]
mod tests;
