use clnr_core::{Safety, human};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::{Group, State, Status};
use crate::theme::Theme;

/// Everything about the item under the cursor.
pub fn draw(frame: &mut Frame, area: Rect, state: &State, theme: &Theme) {
    let block = theme.block().title(" Details ");
    let sep = theme.symbols.sep;
    if let Some(group) = state.current_group() {
        frame.render_widget(
            Paragraph::new(group_lines(group, state, theme)).block(block),
            area,
        );
        return;
    }
    let Some(item) = state.current() else {
        frame.render_widget(Paragraph::new("Nothing selected.").block(block), area);
        return;
    };
    let a = &item.artifact;

    // What it is, then where it really is (when that differs from the project shown).
    let mut lines = vec![Line::styled(human::describe(a, &state.home), theme.accent)];
    for path in a.paths() {
        let path = human::path(path, &state.home);
        if path != human::place(a, &state.home) {
            lines.push(Line::styled(path, theme.dim));
        }
    }

    let size = match a.size {
        Some(s) if a.paths().is_empty() => human::size(s.on_disk),
        Some(s) => format!(
            "{}{sep}{}",
            human::size(s.on_disk),
            human::count(s.entries as usize, "file")
        ),
        None => format!("measuring{}", theme.symbols.ellipsis),
    };
    let changed = a
        .newest_mtime
        .map(|t| format!("{sep}changed {}", human::ago(t)))
        .unwrap_or_default();
    lines.push(Line::raw(format!("{size}{changed}")));

    lines.push(Line::from(vec![
        Span::styled(format!("{}: ", a.safety.as_str()), theme.warn),
        Span::raw(a.safety.description()),
    ]));
    if a.orphaned {
        lines.push(Line::styled(
            "orphan: the project it belongs to no longer exists.",
            theme.warn,
        ));
    }
    if item.status == Status::Failed {
        lines.push(Line::from(Span::styled(
            "The last delete failed; press e for details.",
            theme.error,
        )));
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// A project's location, then one line per member.
fn group_lines<'a>(group: &'a Group, state: &State, theme: &Theme) -> Vec<Line<'a>> {
    let mut lines = vec![Line::styled(group.name.as_str(), theme.accent)];
    lines.extend(
        group
            .members
            .iter()
            .filter_map(|id| state.item(*id))
            .map(|it| {
                let a = &it.artifact;
                let line = format!(
                    "{:>7}  {} {}",
                    human::size(a.on_disk()),
                    a.provider,
                    a.label
                );
                if a.safety == Safety::Caution {
                    Line::from(vec![Span::raw(line), Span::styled("  caution", theme.warn)])
                } else {
                    Line::raw(line)
                }
            }),
    );
    lines
}
