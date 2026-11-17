use clnr_core::human;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::state::{Mode, State};
use crate::theme::Theme;

/// Key hints, most useful first; as many as fit are shown.
const HINTS: &[(&str, &str)] = &[
    ("?", "help"),
    ("space", "select"),
    ("d", "delete"),
    ("q", "quit"),
    ("/", "filter"),
    ("a", "all-safe"),
    ("s", "sort"),
    ("v", "projects"),
    ("enter", "details"),
];

/// One line: the filter prompt, a toast, or the selection summary plus key hints.
pub fn draw(frame: &mut Frame, area: Rect, state: &State, theme: &Theme) {
    let sep = theme.symbols.sep;
    let line = if matches!(state.mode, Mode::Filter) {
        Line::from(vec![
            Span::styled(
                format!("/{}{}", state.query, theme.symbols.caret),
                theme.accent,
            ),
            Span::styled(format!("  enter apply{sep}esc clear"), theme.dim),
        ])
    } else if let Some(toast) = &state.toast {
        Line::styled(toast.as_str(), theme.warn)
    } else {
        let mut spans = Vec::new();
        if !state.selected.is_empty() {
            spans.push(Span::styled(
                format!(
                    "{} selected{sep}{}   ",
                    state.selected.len(),
                    human::size(state.selected_total())
                ),
                theme.marked,
            ));
        }
        if !state.errors.is_empty() {
            spans.push(Span::styled(
                format!("{} (e)   ", human::count(state.errors.len(), "error")),
                theme.error,
            ));
        }
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        let room = (area.width as usize).saturating_sub(used);
        spans.push(Span::styled(hints(room, sep), theme.dim));
        Line::from(spans)
    };
    frame.render_widget(Paragraph::new(line), area);
}

/// `? help · space select · …`, cut at the last hint that fits `width`.
fn hints(width: usize, sep: &str) -> String {
    let mut out = String::new();
    for (key, what) in HINTS {
        let hint = format!("{key} {what}");
        let extra = if out.is_empty() { 0 } else { sep.width() };
        if out.width() + extra + hint.width() > width {
            break;
        }
        if !out.is_empty() {
            out.push_str(sep);
        }
        out.push_str(&hint);
    }
    out
}
