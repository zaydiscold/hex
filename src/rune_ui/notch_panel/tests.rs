//! The notch panel driven offscreen: clicks, keys, and the pointer leaving.
//! `HEX_NOTCH_SNAPS=<folder>` also writes each look as a PNG to look at.

use std::time::Duration;

use rune::text::TextSystem;
use rune::{Env, Key, KeyEvent, Mods, Runtime};

use super::clock::Mode;
use super::geometry::{Look, PANEL_H, PANEL_W};
use super::*;

fn runtime() -> Runtime<Panel> {
    let mut panel = Panel::new(None);
    panel.enabled = true;
    let env = Env {
        system_dark: false,
        scale: 2.0,
    };
    let text = TextSystem::new(rune_theme_opencode::FONTS, rune_theme_opencode::FAMILIES);
    let mut rt = crate::rune_ui::offscreen::runtime(panel, view, (PANEL_W, PANEL_H), env, text);
    rt.frame(0.0);
    rt
}

fn settle(rt: &mut Runtime<Panel>) {
    rt.settle_all();
    rt.frame(0.0);
}

fn tap(rt: &mut Runtime<Panel>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    settle(rt);
}

fn texts(rt: &Runtime<Panel>) -> Vec<String> {
    rt.elements().into_iter().filter_map(|e| e.text).collect()
}

/// A key as the overlay hears it (`on_key`).
fn press_key(rt: &mut Runtime<Panel>, key: Key) {
    rt.update(move |p, tx| {
        on_key(
            p,
            &KeyEvent {
                key,
                mods: Mods::default(),
            },
            tx,
        );
    });
    settle(rt);
}

/// One poll, as Rune runs before each overlay frame.
fn poll_once(rt: &mut Runtime<Panel>) {
    rt.update(|p, tx| {
        poll(p, 0.0, tx);
    });
    settle(rt);
}

fn open(rt: &mut Runtime<Panel>) {
    tap(rt, "island");
    assert!(rt.state.open, "a click on the notch opens it");
}

#[test]
fn a_click_on_the_notch_opens_the_panel_and_esc_closes_it() {
    let mut rt = runtime();
    assert_eq!(rt.state.look(), Look::Bare);
    assert!(rt.find("time").is_none());
    open(&mut rt);
    assert!(rt.find("time").is_some());
    assert!(rt.find("task").is_some());
    press_key(&mut rt, Key::Escape);
    assert!(!rt.state.open);
}

#[test]
fn start_runs_the_stopwatch_and_the_closed_notch_shows_it() {
    let mut rt = runtime();
    open(&mut rt);
    tap(&mut rt, "start");
    assert!(rt.state.clock.stopwatch.running());
    rt.update(|p, tx| p.collapse(tx));
    settle(&mut rt);
    assert_eq!(rt.state.look(), Look::Compact(Mode::Stopwatch));
    assert!(rt.find("compact-time").is_some());
    // The HUD's ears take the notch while it shows.
    rt.update(|p, _| p.dictating = true);
    settle(&mut rt);
    assert_eq!(rt.state.look(), Look::Bare);
}

#[test]
fn the_timer_tab_offers_presets_that_set_its_length() {
    let mut rt = runtime();
    open(&mut rt);
    assert!(
        rt.find("preset-5").is_none(),
        "the stopwatch has no presets"
    );
    tap(&mut rt, "tab-timer");
    assert_eq!(rt.state.clock.mode, Mode::Timer);
    tap(&mut rt, "preset-5");
    assert_eq!(rt.state.clock.timer_ms, 5 * 60_000);
    assert!(texts(&rt).contains(&"5:00".to_string()));
    tap(&mut rt, "plus");
    assert_eq!(rt.state.clock.timer_ms, 6 * 60_000);
    tap(&mut rt, "minus");
    tap(&mut rt, "minus");
    assert_eq!(rt.state.clock.timer_ms, 4 * 60_000);
}

#[test]
fn a_finished_countdown_rings_and_opening_answers_it() {
    let mut rt = runtime();
    rt.update(|p, _| {
        p.clock.mode = Mode::Timer;
        p.clock.timer_ms = 60_000;
        p.clock.timer.since_ms = Some(Panel::now() - 61_000);
    });
    poll_once(&mut rt);
    assert!(rt.state.clock.rang);
    assert_eq!(rt.state.look(), Look::Compact(Mode::Timer));
    assert!(texts(&rt).contains(&"Done".to_string()));
    open(&mut rt);
    assert!(!rt.state.clock.rang, "looking answers the ring");
}

#[test]
fn the_task_check_marks_a_task_done() {
    let mut rt = runtime();
    open(&mut rt);
    rt.update(|p, _| p.task = TextEdit::new("Write the report"));
    tap(&mut rt, "task-check");
    assert_eq!(rt.state.clock.task, "Write the report");
    assert!(rt.state.clock.task_done);
    tap(&mut rt, "task-check");
    assert!(!rt.state.clock.task_done);
}

#[test]
fn an_open_panel_closes_after_the_pointer_leaves() {
    let mut rt = runtime();
    open(&mut rt);
    let (x, y) = rt.find("island").unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.0);
    rt.pointer_leave();
    rt.frame(0.0);
    assert!(rt.state.open, "it waits a moment");
    rt.update(|p, _| p.left_at = Some(Instant::now() - LEAVE_GRACE - Duration::from_millis(1)));
    poll_once(&mut rt);
    assert!(!rt.state.open);
}

#[test]
fn space_starts_and_pauses_the_shown_clock() {
    let mut rt = runtime();
    open(&mut rt);
    press_key(&mut rt, Key::Text(" ".into()));
    assert!(rt.state.clock.stopwatch.running());
    press_key(&mut rt, Key::Text(" ".into()));
    assert!(!rt.state.clock.stopwatch.running());
}

/// Each look as a PNG in `HEX_NOTCH_SNAPS`, to look at by eye.
#[test]
fn notch_snapshots() {
    use rune::render::Gpu;
    let Some(folder) = std::env::var_os("HEX_NOTCH_SNAPS") else {
        return;
    };
    let Ok(gpu) = Gpu::try_new(Gpu::instance(), None) else {
        eprintln!("skipping notch snapshots: no GPU adapter");
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    std::fs::create_dir_all(&folder).unwrap();
    type Setup = fn(&mut Panel);
    let looks: [(&str, Setup); 7] = [
        ("bare", |_| {}),
        ("peek", |p| p.hovered = true),
        ("compact-stopwatch", |p| {
            p.clock.stopwatch.since_ms = Some(Panel::now() - 754_000);
        }),
        ("compact-timer", |p| {
            p.clock.mode = Mode::Timer;
            p.clock.timer.since_ms = Some(Panel::now() - 9 * 60_000);
        }),
        ("compact-done", |p| {
            p.clock.mode = Mode::Timer;
            p.clock.rang = true;
        }),
        ("open-stopwatch", |p| {
            p.open = true;
            p.clock.stopwatch.since_ms = Some(Panel::now() - 754_000);
            p.task = TextEdit::new("Ship the notch panel");
        }),
        ("open-timer", |p| {
            p.open = true;
            p.clock.mode = Mode::Timer;
            p.clock.timer.banked_ms = 4 * 60_000 + 12_000;
        }),
    ];
    for (name, setup) in looks {
        let mut rt = runtime();
        // As the overlay draws: only the island, over a clear window.
        rt.transparent = true;
        rt.update(move |p, _| setup(p));
        let (w, h) = ((PANEL_W * 2.0) as u32, (PANEL_H * 2.0) as u32);
        let pixels = crate::rune_ui::offscreen::draw(&gpu, &mut rt, w, h);
        let path = folder.join(format!("notch-{name}.png"));
        rune::render::write_png(&path, w, h, &pixels).unwrap();
    }
}
