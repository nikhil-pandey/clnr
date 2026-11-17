use clnr_core::human;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::state::{Focus, State};
use crate::theme::Theme;

/// "All", then each provider that found something, biggest first.
pub fn draw(frame: &mut Frame, area: Rect, state: &State, theme: &Theme) {
    // Inside the borders: marker, space, name, padding, size.
    let width = area.width.saturating_sub(2) as usize;
    let lines: Vec<Line> = state
        .sidebar_entries()
        .into_iter()
        .map(|(provider, total)| {
            let picked = provider == state.provider;
            let name = provider.unwrap_or("All");
            let size = human::size(total);
            let marker = if picked { theme.symbols.pointer } else { " " };
            let pad = width.saturating_sub(name.width() + size.width() + 3);
            let text = format!("{marker} {name}{}{size}", " ".repeat(pad));
            let style = match (picked, state.focus) {
                (true, Focus::Sidebar) => theme.cursor,
                (true, Focus::Table) => theme.accent,
                (false, _) => Style::default(),
            };
            Line::styled(text, style)
        })
        .collect();

    let border = if state.focus == Focus::Sidebar {
        theme.accent
    } else {
        Style::default()
    };
    let block = theme.block().title(" Providers ").border_style(border);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
