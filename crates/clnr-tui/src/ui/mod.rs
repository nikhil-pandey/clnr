//! Rendering. Reads [`State`]; the only thing it writes back is layout feedback (page size,
//! where rows are on screen, the table's scroll offset).

mod detail;
mod footer;
mod popup;
mod sidebar;
mod table;
mod text;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::state::{Mode, State};
use crate::theme::Theme;

const SIDEBAR_WIDTH: u16 = 28;
/// Below this terminal width the sidebar is hidden to leave room for the table.
const SIDEBAR_MIN_SCREEN: u16 = 100;
const DETAIL_HEIGHT: u16 = 7;
/// Smallest screen the layout works in.
const MIN_WIDTH: u16 = 40;
const MIN_HEIGHT: u16 = 8;

pub fn draw(frame: &mut Frame, state: &mut State, theme: &Theme) {
    let screen = frame.area();
    if screen.width < MIN_WIDTH || screen.height < MIN_HEIGHT {
        too_small(frame, screen, theme);
        return;
    }

    // The details pane takes at most a third of the height, and only if the table keeps room.
    let detail_height = if state.detail {
        DETAIL_HEIGHT.min(screen.height / 3)
    } else {
        0
    };
    let [main, detail_area, footer_area] = Layout::vertical([
        Constraint::Min(5),
        Constraint::Length(detail_height),
        Constraint::Length(1),
    ])
    .areas(screen);
    let sidebar_width = if screen.width >= SIDEBAR_MIN_SCREEN {
        SIDEBAR_WIDTH
    } else {
        0
    };
    let [sidebar_area, table_area] =
        Layout::horizontal([Constraint::Length(sidebar_width), Constraint::Min(30)]).areas(main);

    state.sidebar_area = sidebar_area;
    if sidebar_width > 0 {
        sidebar::draw(frame, sidebar_area, state, theme);
    }
    table::draw(frame, table_area, state, theme);
    if detail_height > 2 {
        detail::draw(frame, detail_area, state, theme);
    }
    footer::draw(frame, footer_area, state, theme);

    match &state.mode {
        Mode::Confirm(confirm) => popup::confirm(frame, state, confirm, theme),
        Mode::Help => popup::help(frame, theme),
        Mode::Errors => popup::errors(frame, state, theme),
        Mode::Normal | Mode::Filter => {}
    }
}

fn too_small(frame: &mut Frame, screen: Rect, theme: &Theme) {
    let lines = vec![
        Line::styled("Terminal too small", theme.warn),
        Line::raw(format!(
            "{}x{}, need {MIN_WIDTH}x{MIN_HEIGHT}",
            screen.width, screen.height
        )),
        Line::styled("q to quit", theme.dim),
    ];
    let area = screen.centered(Constraint::Length(screen.width), Constraint::Length(3));
    frame.render_widget(Paragraph::new(lines).centered(), area);
}

fn spinner(state: &State, theme: &Theme) -> &'static str {
    let frames = theme.symbols.spinner;
    frames[state.spinner % frames.len()]
}
