//! The notch panel's stopwatch, countdown timer, and task: pure state over
//! wall-clock milliseconds, so a running clock keeps counting across a quit,
//! a restart, or sleep, and every rule here is unit-tested without a screen.
//! The panel saves it to `notch-panel.json` in Hex's Application Support
//! folder after every change.

use serde::{Deserialize, Serialize};

/// The longest countdown the panel offers: ten hours.
pub(crate) const MAX_TIMER_MS: i64 = 10 * 60 * 60 * 1000;
/// The shortest: one minute.
pub(crate) const MIN_TIMER_MS: i64 = 60 * 1000;
/// A new countdown's length: a 25-minute focus block.
pub(crate) const DEFAULT_TIMER_MS: i64 = 25 * 60 * 1000;
/// The longest task the panel keeps, in characters.
pub(crate) const MAX_TASK_CHARS: usize = 200;

/// Which clock the panel shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    #[default]
    Stopwatch,
    Timer,
}

/// Time that accumulates while running: what earlier runs added, and when
/// the current run started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Run {
    /// Milliseconds counted by runs that have ended.
    #[serde(default)]
    pub(crate) banked_ms: i64,
    /// When the current run started, wall-clock milliseconds; `None` paused.
    #[serde(default)]
    pub(crate) since_ms: Option<i64>,
}

impl Run {
    /// Everything counted by `now`. A clock set back never counts backwards.
    pub(crate) fn elapsed(self, now: i64) -> i64 {
        self.banked_ms + self.since_ms.map_or(0, |since| (now - since).max(0))
    }

    pub(crate) fn running(self) -> bool {
        self.since_ms.is_some()
    }

    fn start(&mut self, now: i64) {
        if self.since_ms.is_none() {
            self.since_ms = Some(now);
        }
    }

    fn pause(&mut self, now: i64) {
        self.banked_ms = self.elapsed(now);
        self.since_ms = None;
    }
}

/// Everything the panel keeps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Clock {
    #[serde(default)]
    pub(crate) mode: Mode,
    #[serde(default)]
    pub(crate) stopwatch: Run,
    #[serde(default)]
    pub(crate) timer: Run,
    /// The countdown's length.
    #[serde(default = "default_timer_ms")]
    pub(crate) timer_ms: i64,
    /// The countdown reached zero and nobody has looked yet.
    #[serde(default)]
    pub(crate) rang: bool,
    /// What the user is working on.
    #[serde(default)]
    pub(crate) task: String,
    #[serde(default)]
    pub(crate) task_done: bool,
}

fn default_timer_ms() -> i64 {
    DEFAULT_TIMER_MS
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            stopwatch: Run::default(),
            timer: Run::default(),
            timer_ms: DEFAULT_TIMER_MS,
            rang: false,
            task: String::new(),
            task_done: false,
        }
    }
}

/// What one tick of the clock found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tick {
    Quiet,
    /// The countdown just reached zero: ring once.
    Rang,
}

impl Clock {
    /// A saved clock made safe to use: a countdown length in range, a task
    /// no longer than the panel keeps.
    pub(crate) fn sanitized(mut self) -> Self {
        self.timer_ms = self.timer_ms.clamp(MIN_TIMER_MS, MAX_TIMER_MS);
        self.stopwatch.banked_ms = self.stopwatch.banked_ms.max(0);
        self.timer.banked_ms = self.timer.banked_ms.clamp(0, self.timer_ms);
        self.task = clamp_task(&self.task);
        self
    }

    /// The shown clock's run.
    fn run(&self) -> Run {
        match self.mode {
            Mode::Stopwatch => self.stopwatch,
            Mode::Timer => self.timer,
        }
    }

    fn run_mut(&mut self) -> &mut Run {
        match self.mode {
            Mode::Stopwatch => &mut self.stopwatch,
            Mode::Timer => &mut self.timer,
        }
    }

    /// Whether the shown clock runs.
    pub(crate) fn running(&self) -> bool {
        self.run().running()
    }

    /// The clock the collapsed notch shows: the countdown while it runs or
    /// has rung (it ends, so it wins when both run), else a running
    /// stopwatch; `None` leaves the notch bare.
    pub(crate) fn compact(&self) -> Option<Mode> {
        if self.timer.running() || self.rang {
            return Some(Mode::Timer);
        }
        self.stopwatch.running().then_some(Mode::Stopwatch)
    }

    /// The stopwatch's count by `now`.
    pub(crate) fn stopwatch_ms(&self, now: i64) -> i64 {
        self.stopwatch.elapsed(now)
    }

    /// The countdown's time left by `now`, never below zero.
    pub(crate) fn remaining_ms(&self, now: i64) -> i64 {
        (self.timer_ms - self.timer.elapsed(now)).max(0)
    }

    /// The countdown's progress by `now`, 0 to 1.
    pub(crate) fn timer_progress(&self, now: i64) -> f32 {
        if self.timer_ms <= 0 {
            return 1.0;
        }
        (self.timer.elapsed(now) as f64 / self.timer_ms as f64).clamp(0.0, 1.0) as f32
    }

    /// The milliseconds `mode` shows by `now`: counted up, or left.
    pub(crate) fn shown_ms(&self, mode: Mode, now: i64) -> i64 {
        match mode {
            Mode::Stopwatch => self.stopwatch_ms(now),
            Mode::Timer => self.remaining_ms(now),
        }
    }

    /// Starts the shown clock, or pauses it. A finished countdown starts
    /// over.
    pub(crate) fn toggle(&mut self, now: i64) {
        if self.mode == Mode::Timer && self.remaining_ms(now) == 0 {
            self.timer = Run::default();
            self.rang = false;
        }
        let run = self.run_mut();
        if run.running() {
            run.pause(now);
        } else {
            run.start(now);
        }
    }

    /// Zeroes the shown clock and stops it.
    pub(crate) fn reset(&mut self) {
        *self.run_mut() = Run::default();
        if self.mode == Mode::Timer {
            self.rang = false;
        }
    }

    /// Whether the shown clock has anything to reset.
    pub(crate) fn resettable(&self, now: i64) -> bool {
        let run = self.run();
        run.running() || run.elapsed(now) > 0 || (self.mode == Mode::Timer && self.rang)
    }

    /// Sets the countdown's length. A running or part-run countdown starts
    /// over at the new length, paused unless it was running.
    pub(crate) fn set_timer(&mut self, ms: i64, now: i64) {
        let was_running = self.timer.running();
        self.timer_ms = ms.clamp(MIN_TIMER_MS, MAX_TIMER_MS);
        self.timer = Run::default();
        self.rang = false;
        if was_running {
            self.timer.start(now);
        }
    }

    /// Nudges the countdown's length by `delta` milliseconds, keeping what
    /// a running one already counted, and never past zero left or the
    /// panel's range.
    pub(crate) fn nudge_timer(&mut self, delta: i64, now: i64) {
        let counted = self.timer.elapsed(now);
        let floor = (counted + 1000).max(MIN_TIMER_MS);
        self.timer_ms = (self.timer_ms + delta).clamp(floor.min(MAX_TIMER_MS), MAX_TIMER_MS);
        if self.remaining_ms(now) > 0 {
            self.rang = false;
        }
    }

    /// Advances the countdown: stops it at zero and rings once.
    pub(crate) fn tick(&mut self, now: i64) -> Tick {
        if self.timer.running() && self.timer.elapsed(now) >= self.timer_ms {
            self.timer = Run {
                banked_ms: self.timer_ms,
                since_ms: None,
            };
            self.rang = true;
            return Tick::Rang;
        }
        Tick::Quiet
    }

    /// The user saw the ring.
    pub(crate) fn acknowledge(&mut self) {
        if self.rang {
            self.rang = false;
            self.timer = Run::default();
        }
    }

    /// How long until what the panel shows next changes: the next whole
    /// second of a running clock, or the countdown's end; `None` while
    /// nothing runs.
    pub(crate) fn next_change_ms(&self, now: i64) -> Option<i64> {
        let mut next: Option<i64> = None;
        let mut take = |ms: i64| next = Some(next.map_or(ms, |n: i64| n.min(ms)));
        if self.stopwatch.running() {
            take(1000 - self.stopwatch.elapsed(now).rem_euclid(1000));
        }
        if self.timer.running() {
            let left = self.remaining_ms(now);
            // A countdown shows whole seconds rounded up, so it changes
            // as each second's remainder runs out.
            let into = left.rem_euclid(1000);
            take(if into == 0 { 1000 } else { into }.min(left.max(1)));
        }
        next.map(|ms| ms.max(1))
    }

    pub(crate) fn set_task(&mut self, task: &str) {
        let task = clamp_task(task);
        if task != self.task {
            self.task_done = false;
        }
        self.task = task;
    }
}

/// `task` on one line, trimmed, no longer than the panel keeps.
pub(crate) fn clamp_task(task: &str) -> String {
    let one_line: String = task
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    one_line.trim().chars().take(MAX_TASK_CHARS).collect()
}

/// `ms` as the panel shows it: `m:ss`, or `h:mm:ss` from an hour. A
/// countdown rounds up, so it reads 0:00 only once it ends.
pub(crate) fn format(ms: i64, round_up: bool) -> String {
    let ms = ms.max(0);
    let seconds = if round_up {
        (ms + 999) / 1000
    } else {
        ms / 1000
    };
    let (h, m, s) = (seconds / 3600, (seconds / 60) % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `ms` as a short length for a preset: `5m`, `1h`, `1h 30m`.
pub(crate) fn format_length(ms: i64) -> String {
    let minutes = ms / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i64 = 1000;
    const MIN: i64 = 60 * S;

    #[test]
    fn a_stopwatch_counts_across_pauses() {
        let mut clock = Clock::default();
        clock.toggle(0);
        assert!(clock.running());
        assert_eq!(clock.stopwatch_ms(5 * S), 5 * S);
        clock.toggle(5 * S);
        assert!(!clock.running());
        assert_eq!(clock.stopwatch_ms(60 * S), 5 * S, "paused, it holds");
        clock.toggle(60 * S);
        assert_eq!(clock.stopwatch_ms(62 * S), 7 * S);
        clock.reset();
        assert_eq!(clock.stopwatch_ms(100 * S), 0);
        assert!(!clock.running());
    }

    #[test]
    fn a_clock_set_back_never_counts_backwards() {
        let mut clock = Clock::default();
        clock.toggle(10 * S);
        assert_eq!(clock.stopwatch_ms(4 * S), 0);
    }

    #[test]
    fn a_countdown_rings_once_at_zero_and_stops() {
        let mut clock = Clock {
            mode: Mode::Timer,
            timer_ms: 2 * MIN,
            ..Clock::default()
        };
        clock.toggle(0);
        assert_eq!(clock.remaining_ms(30 * S), 90 * S);
        assert_eq!(clock.tick(119 * S), Tick::Quiet);
        assert_eq!(clock.tick(121 * S), Tick::Rang);
        assert!(clock.rang);
        assert!(!clock.timer.running());
        assert_eq!(clock.remaining_ms(200 * S), 0);
        assert_eq!(clock.tick(300 * S), Tick::Quiet, "it rings only once");
        assert_eq!(clock.compact(), Some(Mode::Timer));
        clock.acknowledge();
        assert!(!clock.rang);
        assert_eq!(
            clock.remaining_ms(300 * S),
            2 * MIN,
            "seen, it's ready again"
        );
        assert_eq!(clock.compact(), None);
    }

    #[test]
    fn a_finished_countdown_starts_over_when_started() {
        let mut clock = Clock {
            mode: Mode::Timer,
            timer_ms: MIN,
            ..Clock::default()
        };
        clock.toggle(0);
        clock.tick(61 * S);
        clock.toggle(70 * S);
        assert!(clock.running());
        assert!(!clock.rang);
        assert_eq!(clock.remaining_ms(70 * S), MIN);
    }

    #[test]
    fn a_countdown_rounds_up_so_zero_means_done() {
        assert_eq!(format(59_001, true), "1:00");
        assert_eq!(format(1, true), "0:01");
        assert_eq!(format(0, true), "0:00");
        assert_eq!(format(59_999, false), "0:59");
        assert_eq!(format(3_600_000, false), "1:00:00");
        assert_eq!(format(3_725_000, false), "1:02:05");
        assert_eq!(format(-5, false), "0:00");
    }

    #[test]
    fn preset_lengths_read_short() {
        assert_eq!(format_length(5 * MIN), "5m");
        assert_eq!(format_length(60 * MIN), "1h");
        assert_eq!(format_length(90 * MIN), "1h 30m");
    }

    #[test]
    fn setting_a_length_restarts_the_countdown_as_it_was() {
        let mut clock = Clock {
            mode: Mode::Timer,
            ..Clock::default()
        };
        clock.toggle(0);
        clock.set_timer(5 * MIN, 10 * S);
        assert!(clock.timer.running(), "a running countdown keeps running");
        assert_eq!(clock.remaining_ms(10 * S), 5 * MIN);
        clock.toggle(20 * S);
        clock.set_timer(45 * MIN, 30 * S);
        assert!(!clock.timer.running(), "a paused one stays paused");
        assert_eq!(clock.remaining_ms(30 * S), 45 * MIN);
        clock.set_timer(0, 30 * S);
        assert_eq!(clock.timer_ms, MIN_TIMER_MS);
        clock.set_timer(i64::MAX, 30 * S);
        assert_eq!(clock.timer_ms, MAX_TIMER_MS);
    }

    #[test]
    fn nudging_keeps_what_a_running_countdown_counted() {
        let mut clock = Clock {
            mode: Mode::Timer,
            timer_ms: 10 * MIN,
            ..Clock::default()
        };
        clock.toggle(0);
        clock.nudge_timer(5 * MIN, 2 * MIN);
        assert_eq!(clock.remaining_ms(2 * MIN), 13 * MIN);
        // Taking away more than is left leaves a second, never a ring.
        clock.nudge_timer(-60 * MIN, 2 * MIN);
        assert_eq!(clock.remaining_ms(2 * MIN), S);
        assert!(clock.timer.running());
    }

    #[test]
    fn the_notch_shows_the_running_clock_and_prefers_the_countdown() {
        let mut clock = Clock::default();
        assert_eq!(clock.compact(), None);
        clock.toggle(0);
        assert_eq!(clock.compact(), Some(Mode::Stopwatch));
        clock.mode = Mode::Timer;
        clock.toggle(0);
        assert_eq!(clock.compact(), Some(Mode::Timer));
        assert!(clock.stopwatch.running() && clock.timer.running());
    }

    #[test]
    fn the_next_change_is_the_next_whole_second() {
        let mut clock = Clock::default();
        assert_eq!(clock.next_change_ms(0), None);
        clock.toggle(0);
        assert_eq!(clock.next_change_ms(250), Some(750));
        assert_eq!(clock.next_change_ms(1000), Some(1000));
        let mut timer = Clock {
            mode: Mode::Timer,
            timer_ms: MIN,
            ..Clock::default()
        };
        timer.toggle(0);
        // 59.7 s left shows 1:00 until 0.7 s pass.
        assert_eq!(timer.next_change_ms(300), Some(700));
        assert_eq!(timer.next_change_ms(59_900), Some(100));
    }

    #[test]
    fn a_task_stays_one_trimmed_line_and_a_new_one_is_not_done() {
        let mut clock = Clock::default();
        clock.set_task("  write the\nreport  ");
        assert_eq!(clock.task, "write the report");
        clock.task_done = true;
        clock.set_task("write the report");
        assert!(clock.task_done, "the same task keeps its check");
        clock.set_task("ship it");
        assert!(!clock.task_done);
        let long = "x".repeat(MAX_TASK_CHARS + 50);
        clock.set_task(&long);
        assert_eq!(clock.task.chars().count(), MAX_TASK_CHARS);
    }

    #[test]
    fn a_saved_clock_round_trips_and_repairs_bad_values() {
        let mut clock = Clock::default();
        clock.toggle(1234);
        clock.set_task("focus");
        let json = serde_json::to_string(&clock).unwrap();
        let back: Clock = serde_json::from_str(&json).unwrap();
        assert_eq!(back, clock);
        let empty: Clock = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Clock::default());
        let bad: Clock =
            serde_json::from_str(r#"{"timer_ms": -4, "timer": {"banked_ms": 99999999}}"#).unwrap();
        let bad = bad.sanitized();
        assert_eq!(bad.timer_ms, MIN_TIMER_MS);
        assert_eq!(bad.timer.banked_ms, MIN_TIMER_MS);
    }
}
