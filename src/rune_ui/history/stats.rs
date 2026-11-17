//! Labs › Dictation stats in History: the strip of totals over the list, and
//! the dialog the header's Stats button opens.

use rune::tok::TextStyle;
use rune::{El, col, el, row, text};

use super::super::Ui;
use super::super::controls::{
    ButtonKind, Paint, button, modal_scrim_transition, modal_transition, raised,
};
use super::super::list_detail::HANG;
use super::{PADDING, STATS_WIDTH};
use crate::desktop::i18n::{tr, tr_fill, tr_named};
use crate::history::stats::HistoryStats;

/// The tallest bar in the strip's chart.
const BAR_HEIGHT: f32 = 30.0;
/// One day's bar, and the gap to the next.
const BAR_WIDTH: f32 = 6.0;
const BAR_GAP: f32 = 3.0;

/// The time dictating saved over typing, in hours and minutes.
fn time_saved(stats: &HistoryStats) -> String {
    let minutes = stats.minutes_saved();
    if minutes < 60 {
        tr_fill("{} min", &minutes.to_string())
    } else {
        tr_named(
            "{hours} h {minutes} min",
            &[
                ("hours", &(minutes / 60).to_string()),
                ("minutes", &(minutes % 60).to_string()),
            ],
        )
    }
}

/// One total: its number large, its name under it.
fn tile(p: Paint, key: &str, value: &str, label: &str) -> El {
    let number = TextStyle {
        size: 22.0,
        line_height: 26.0,
        features: "'tnum' 1",
        ..p.k.ty.body_medium
    };
    col()
        .key(key)
        .gap(2.0)
        .child(text(value).key("value").font(number).color(p.text()))
        .child(text(label).key("label").font(p.k.ty.small).color(p.muted()))
}

/// Words per day as bars, oldest first, today's in the accent colour.
fn chart(p: Paint, days: &[u64]) -> El {
    let most = days.iter().copied().max().unwrap_or(0).max(1);
    let last = days.len().saturating_sub(1);
    let bars = days.iter().enumerate().map(|(index, words)| {
        let height = if *words == 0 {
            2.0
        } else {
            (BAR_HEIGHT * *words as f32 / most as f32).max(3.0)
        };
        el().key(format!("day{index}"))
            .w(BAR_WIDTH)
            .h(height)
            .radius(2.0)
            .fill(if index == last {
                p.k.c.icon_accent
            } else if *words == 0 {
                p.k.c.border_weak
            } else {
                p.k.c.border_strong
            })
    });
    col()
        .key("recent")
        .gap(2.0)
        .items_end()
        .child(
            row()
                .key("bars")
                .h(BAR_HEIGHT)
                .gap(BAR_GAP)
                .items_end()
                .children(bars),
        )
        .child(
            text(tr("Last 14 days"))
                .key("label")
                .font(p.k.ty.small)
                .color(p.muted()),
        )
}

/// The totals over History's list, while Labs › Dictation stats is on and
/// something is retained.
pub(super) fn strip(ui: &Ui, p: Paint) -> Option<El> {
    if !ui.settings.labs.history_stats {
        return None;
    }
    let stats = ui
        .history
        .stats
        .as_ref()
        .filter(|stats| stats.dictations > 0)?;
    let wpm = stats
        .words_per_minute
        .map_or_else(|| "–".to_owned(), |wpm| wpm.to_string());
    let tiles = row()
        .key("totals")
        .gap(28.0)
        .flex_wrap()
        .child(tile(p, "words", &stats.words.to_string(), tr("Words")))
        .child(tile(
            p,
            "dictations",
            &stats.dictations.to_string(),
            tr("Dictations"),
        ))
        .child(tile(p, "speed", &wpm, tr("Words per minute")))
        .child(tile(p, "saved", &time_saved(stats), tr("Time saved")))
        .child(tile(
            p,
            "streak",
            &stats.streak_days.to_string(),
            tr("Daily streak"),
        ));
    Some(
        raised(p, "history-stats-strip")
            .role("group")
            .label(tr("Dictation stats"))
            .mx(HANG)
            .mb(12.0)
            .px(PADDING)
            .py(14.0)
            .child(
                row()
                    .key("content")
                    .gap(24.0)
                    .items_end()
                    .justify_between()
                    .child(tiles)
                    .children(
                        (!stats.recent_words.is_empty()).then(|| chart(p, &stats.recent_words)),
                    ),
            ),
    )
}

/// Labs › Dictation stats: every retained dictation summed up, in the
/// dialog the header's Stats button opens.
fn stats_lines(p: Paint, stats: &HistoryStats) -> Vec<El> {
    let wpm = stats
        .words_per_minute
        .map_or_else(|| "–".to_owned(), |wpm| wpm.to_string());
    let mut lines = vec![
        tr_named(
            "Dictations: {count} · Words: {words} · Words per minute: {wpm}",
            &[
                ("count", &stats.dictations.to_string()),
                ("words", &stats.words.to_string()),
                ("wpm", &wpm),
            ],
        ),
        tr_named(
            "Changed by Output: {percent}% · Daily streak: {days}",
            &[
                (
                    "percent",
                    &stats.changed_percent().unwrap_or_default().to_string(),
                ),
                ("days", &stats.streak_days.to_string()),
            ],
        ),
    ];
    if !stats.top_apps.is_empty() {
        let apps = stats
            .top_apps
            .iter()
            .map(|app| format!("{} ({})", app.application, app.dictations))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(tr_fill("Top apps: {}", &apps));
    }
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            text(line)
                .key(format!("line{index}"))
                .font(super::super::controls::DESCRIPTION)
                .color(p.muted())
                .wrap()
        })
        .collect()
}

/// The stats dialog's scrim and panel, while its button and stats show.
pub(super) fn stats_modal(ui: &Ui, p: Paint) -> Option<[El; 2]> {
    if !ui.history.stats_open || !ui.settings.labs.history_stats {
        return None;
    }
    let stats = ui
        .history
        .stats
        .as_ref()
        .filter(|stats| stats.dictations > 0)?;
    let c = &p.k.c;
    let mut panel = col()
        .key("history-stats-dialog")
        .role("dialog")
        .label(tr("Dictation stats"))
        .w(STATS_WIDTH)
        .pt(PADDING)
        .px(PADDING)
        .pb(14.0)
        .gap(12.0)
        .radius(p.k.radius.xl)
        .fill(c.bg_base)
        .border(p.hair, c.border_weak)
        // Presses inside the panel stay inside.
        .on_press(|_: &mut Ui| {})
        .child(
            text(tr("Dictation stats"))
                .key("title")
                .font(p.k.ty.body_medium)
                .color(p.text()),
        )
        .children(stats_lines(p, stats))
        .child(
            row().key("footer").justify_end().child(
                button(
                    p,
                    "history-stats-close",
                    tr("Close"),
                    ButtonKind::Secondary,
                    |ui: &mut Ui, _| close_stats(ui),
                )
                .default_button(),
            ),
        );
    for shadow in &p.k.elevation.overlay {
        panel = panel.box_shadow(*shadow);
    }
    let layer = |key: &str| {
        el().key(key)
            .role("none")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
    };
    Some([
        layer("history-stats-scrim")
            .fill(c.scrim)
            .on_press(close_stats)
            .transition(modal_scrim_transition(p)),
        layer("history-stats-layer")
            .center()
            .transition(modal_transition(p))
            // Keys stay inside, Escape or a press outside closes, and focus
            // returns to the Stats button that opened it.
            .child(panel.modal(true, close_stats)),
    ])
}

/// Closes the stats dialog, from its button, Escape, or the scrim.
pub(in crate::rune_ui) fn close_stats(ui: &mut Ui) {
    ui.history.stats_open = false;
}
