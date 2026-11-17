use clnr_core::human;
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use crate::state::{Confirm, State};
use crate::theme::Theme;

/// Items listed by name in the confirm dialog before summarizing the rest.
const CONFIRM_LISTED: usize = 8;

const HELP: &[(&str, &str)] = &[
    ("j / k, up / down", "move"),
    ("g / G, PgUp / PgDn", "jump"),
    ("tab", "switch between providers and artifacts"),
    (
        "space",
        "select / unselect (a project: its non-caution items)",
    ),
    ("a / A", "select all non-caution / clear selection"),
    ("/", "filter by text; esc clears"),
    ("s / S", "cycle sort / reverse"),
    ("enter", "details"),
    ("v", "toggle view: artifacts / projects"),
    ("d", "delete selection (or current row)"),
    ("r", "rescan"),
    ("e", "errors"),
    ("q, ctrl-c", "quit"),
    (
        "mouse",
        "wheel scrolls; click a row to move there, its first column to select",
    ),
];

/// Preferred popup widths; never wider than the screen.
const WIDE: u16 = 100;
const NARROW: u16 = 76;

pub fn confirm(frame: &mut Frame, state: &State, confirm: &Confirm, theme: &Theme) {
    let heading = if confirm.allowed.is_empty() {
        "Nothing can be deleted.".to_owned()
    } else {
        format!(
            "Permanently delete {} ({})?",
            human::count(confirm.allowed.len(), "item"),
            human::size(confirm.total)
        )
    };
    let mut lines = vec![Line::from(Span::styled(heading, theme.accent))];
    lines.push(Line::raw(""));

    let path_of = |id| {
        state
            .item(id)
            .map(|it| human::describe(&it.artifact, &state.home))
            .unwrap_or_default()
    };
    for id in confirm.allowed.iter().take(CONFIRM_LISTED) {
        let size = state
            .item(*id)
            .map(|it| human::size(it.artifact.on_disk()))
            .unwrap_or_default();
        lines.push(Line::raw(format!("  {size:>7}  {}", path_of(*id))));
    }
    if confirm.allowed.len() > CONFIRM_LISTED {
        lines.push(Line::raw(format!(
            "  {} and {} more",
            theme.symbols.ellipsis,
            confirm.allowed.len() - CONFIRM_LISTED
        )));
    }
    if !confirm.blocked.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!("Skipping {}:", human::count(confirm.blocked.len(), "item")),
            theme.warn,
        )));
        for (id, reason) in &confirm.blocked {
            lines.push(Line::raw(format!("  {}: {reason}", path_of(*id))));
        }
    }
    if state.interrupted > 0 && !confirm.allowed.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            format!(
                "Also finishes {} interrupted earlier.",
                human::count(state.interrupted, "delete")
            ),
            theme.dim,
        )));
    }
    lines.push(Line::raw(""));
    let prompt = if confirm.allowed.is_empty() {
        Line::from(Span::styled("esc to close", theme.dim))
    } else if confirm.needs_yes {
        Line::from(vec![
            Span::styled(
                "Includes caution items. Type yes and press enter: ",
                theme.warn,
            ),
            Span::styled(
                format!("{}{}", confirm.typed, theme.symbols.caret),
                theme.accent,
            ),
        ])
    } else {
        Line::from(Span::styled(
            format!("y / enter delete{}n / esc cancel", theme.symbols.sep),
            theme.dim,
        ))
    };
    lines.push(prompt);

    show(frame, " Delete ", lines, WIDE, theme);
}

pub fn help(frame: &mut Frame, theme: &Theme) {
    let lines = HELP
        .iter()
        .map(|(keys, what)| {
            Line::from(vec![
                Span::styled(format!("  {keys:<20}"), theme.accent),
                Span::raw(*what),
            ])
        })
        .collect();
    show(frame, " Keys ", lines, NARROW, theme);
}

pub fn errors(frame: &mut Frame, state: &State, theme: &Theme) {
    let lines = if state.errors.is_empty() {
        vec![Line::from(Span::styled("No errors.", theme.dim))]
    } else {
        state.errors.iter().map(|e| Line::raw(e.as_str())).collect()
    };
    show(frame, " Errors ", lines, WIDE, theme);
}

/// A bordered popup sized to its (wrapped) content, centered, at most `max_width` wide and
/// never larger than the screen.
fn show(frame: &mut Frame, title: &str, lines: Vec<Line>, max_width: u16, theme: &Theme) {
    let screen = frame.area();
    let width = max_width.min(screen.width.saturating_sub(2));
    let inner_width = width.saturating_sub(2).max(1) as usize;
    let rows: usize = lines
        .iter()
        .map(|l| l.width().max(1).div_ceil(inner_width))
        .sum();
    let height = (rows as u16)
        .saturating_add(2)
        .min(screen.height.saturating_sub(2));

    let area: Rect = screen.centered(Constraint::Length(width), Constraint::Length(height));
    frame.render_widget(Clear, area);
    let paragraph = Paragraph::new(lines)
        .block(theme.block().title(title.to_owned()))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}
