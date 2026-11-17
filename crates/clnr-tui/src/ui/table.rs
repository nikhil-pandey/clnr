use clnr_core::{Safety, human};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Cell, Row, Table};

use super::{spinner, text};
use crate::state::{Focus, Group, Item, State, Status, View};
use crate::theme::Theme;

/// Table widths below this drop one column per view (provider, or project location).
const COMPACT_BELOW: u16 = 70;
/// Gap between columns; the `Table` widget's default.
const COLUMN_SPACING: u16 = 1;

/// How a column's text is fitted when it is too wide.
#[derive(Clone, Copy)]
enum Fit {
    /// Keep the start (labels, names).
    Start,
    /// Keep the end (paths).
    End,
}

struct Column {
    header: &'static str,
    width: Constraint,
    fit: Fit,
    right: bool,
}

const fn col(header: &'static str, width: Constraint) -> Column {
    Column {
        header,
        width,
        fit: Fit::Start,
        right: false,
    }
}

const fn path(header: &'static str, width: Constraint) -> Column {
    Column {
        fit: Fit::End,
        ..col(header, width)
    }
}

const fn num(header: &'static str, width: u16) -> Column {
    Column {
        right: true,
        ..col(header, Constraint::Length(width))
    }
}

/// What it is, whose it is, how big, how stale, and anything that should give pause.
const ARTIFACT_COLUMNS: [Column; 7] = [
    col("", Constraint::Length(1)),
    col("TOOL", Constraint::Length(9)),
    col("WHAT", Constraint::Fill(1)),
    // The project or owner (paths keep their tail), or a Docker object's name.
    path("WHERE", Constraint::Fill(2)),
    num("SIZE", 7),
    num("CHANGED", 8),
    col("NOTE", Constraint::Length(9)),
];
const ARTIFACT_OPTIONAL: usize = 1;

const PROJECT_COLUMNS: [Column; 6] = [
    col("", Constraint::Length(1)),
    path("PROJECT", Constraint::Fill(2)),
    col("CONTAINS", Constraint::Fill(2)),
    num("SIZE", 7),
    num("CHANGED", 8),
    col("NOTE", Constraint::Length(6)),
];
const PROJECT_OPTIONAL: usize = 2;

/// A cell's text and style before fitting.
type Text = (String, Style);

pub fn draw(frame: &mut Frame, area: Rect, state: &mut State, theme: &Theme) {
    let border = if state.focus == Focus::Table {
        theme.accent
    } else {
        Style::default()
    };
    let block = theme
        .block()
        .title(title(state, theme))
        .border_style(border);
    let inner = block.inner(area);
    // Layout feedback: page size and where rows are, for paging and mouse clicks.
    let rows_area = Rect {
        y: inner.y + 1,
        height: inner.height.saturating_sub(1),
        ..inner
    };
    state.page = rows_area.height as usize;
    state.rows_area = rows_area;

    let compact = area.width < COMPACT_BELOW;
    let (all, optional): (&[Column], usize) = match state.view {
        View::Flat => (&ARTIFACT_COLUMNS, ARTIFACT_OPTIONAL),
        View::Projects => (&PROJECT_COLUMNS, PROJECT_OPTIONAL),
    };
    let columns = keep(all.iter().collect(), compact, optional);
    let constraints: Vec<Constraint> = columns.iter().map(|c| c.width).collect();
    let widths: Vec<usize> = Layout::horizontal(constraints.clone())
        .spacing(COLUMN_SPACING)
        .split(inner)
        .iter()
        .map(|r| r.width as usize)
        .collect();

    let texts: Vec<(Vec<Text>, Style)> = match state.view {
        View::Flat => state
            .rows()
            .map(|it| artifact_texts(it, state, theme))
            .collect(),
        View::Projects => state
            .groups()
            .iter()
            .map(|g| project_texts(g, state, theme))
            .collect(),
    };
    let rows =
        texts.into_iter().map(|(cells, style)| {
            let cells = keep(cells, compact, optional);
            Row::new(
                cells.into_iter().zip(&columns).zip(&widths).map(
                    |(((text, style), column), &width)| fit(column, &text, style, width, theme),
                ),
            )
            .style(style)
        });
    let header = columns
        .iter()
        .map(|c| fit(c, c.header, Style::default(), usize::MAX, theme));

    let table = Table::new(rows, constraints)
        .column_spacing(COLUMN_SPACING)
        .header(Row::new(header).style(theme.dim))
        .block(block)
        .row_highlight_style(theme.cursor);

    let mut table_state = std::mem::take(&mut state.table);
    table_state.select((state.visible_len() > 0).then_some(state.cursor));
    frame.render_stateful_widget(table, area, &mut table_state);
    state.table = table_state;
}

fn fit(column: &Column, text: &str, style: Style, width: usize, theme: &Theme) -> Cell<'static> {
    let ellipsis = theme.symbols.ellipsis;
    let text = match column.fit {
        Fit::Start => text::truncate_right(text, width, ellipsis),
        Fit::End => text::truncate_left(text, width, ellipsis),
    };
    let line = Line::styled(text, style);
    Cell::from(if column.right {
        line.right_aligned()
    } else {
        line
    })
}

fn title(state: &State, theme: &Theme) -> String {
    let s = theme.symbols;
    let arrow = if state.reverse { s.up } else { s.down };
    let what = match state.view {
        View::Flat => "Artifacts",
        View::Projects => "Projects",
    };
    let sep = s.sep;
    let mut title = format!(
        " {what}{sep}{}{sep}sort {} {arrow}",
        state.visible_len(),
        state.sort.label()
    );
    if !state.query.is_empty() {
        title += &format!("{sep}/{}", state.query);
    }
    if state.scanning {
        title += &format!("{sep}scanning {}", spinner(state, theme));
    }
    title.push(' ');
    title
}

/// All columns, or all but `optional` when compact.
fn keep<T>(all: Vec<T>, compact: bool, optional: usize) -> Vec<T> {
    all.into_iter()
        .enumerate()
        .filter(|(i, _)| !(compact && *i == optional))
        .map(|(_, c)| c)
        .collect()
}

fn plain(text: impl Into<String>) -> Text {
    (text.into(), Style::default())
}

fn artifact_texts(item: &Item, state: &State, theme: &Theme) -> (Vec<Text>, Style) {
    let a = &item.artifact;
    let marked = state.selected.contains(&a.id);
    let size = a.size.map_or_else(
        || spinner(state, theme).to_owned(),
        |s| human::size(s.on_disk),
    );
    let note = match item.status {
        Status::Deleting => (format!("deleting{}", theme.symbols.ellipsis), theme.dim),
        Status::Failed => ("failed".to_owned(), theme.error),
        Status::Idle if a.orphaned => ("orphan".to_owned(), theme.warn),
        Status::Idle if a.safety == Safety::Caution => ("caution".to_owned(), theme.warn),
        Status::Idle => plain(""),
    };
    let style = if item.status == Status::Deleting {
        theme.dim
    } else if marked {
        theme.marked
    } else {
        Style::default()
    };
    let cells = vec![
        plain(if marked { theme.symbols.marked } else { " " }),
        plain(a.provider),
        plain(a.label.as_str()),
        plain(human::place(a, &state.home)),
        plain(size),
        plain(a.newest_mtime.map(human::ago).unwrap_or_default()),
        note,
    ];
    (cells, style)
}

fn project_texts(group: &Group, state: &State, theme: &Theme) -> (Vec<Text>, Style) {
    let selected = group
        .members
        .iter()
        .filter(|id| state.selected.contains(id))
        .count();
    let mark = match selected {
        0 => " ",
        n if n == group.members.len() => theme.symbols.marked,
        _ => theme.symbols.partial,
    };
    let style = if selected > 0 {
        theme.marked
    } else {
        Style::default()
    };
    let note = if group.orphaned {
        ("orphan".to_owned(), theme.warn)
    } else {
        plain("")
    };
    let cells = vec![
        plain(mark),
        plain(group.name.as_str()),
        plain(group.kinds.join(", ")),
        plain(human::size(group.total)),
        plain(group.newest.map(human::ago).unwrap_or_default()),
        note,
    ];
    (cells, style)
}
