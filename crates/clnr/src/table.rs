//! Tables that fit the terminal: long cells wrap, low-priority columns are hidden on narrow
//! terminals, and empty columns are hidden. Piped output is never wrapped.

use comfy_table::{
    Attribute, Cell, CellAlignment, Color, ColumnConstraint, ContentArrangement, Table, presets,
};

use crate::style;

pub struct Column {
    pub name: &'static str,
    pub align: Align,
    /// Hidden when the terminal is narrower than this. `0` means always shown.
    pub min_width: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

impl Column {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            align: Align::Left,
            min_width: 0,
        }
    }

    pub const fn right(self) -> Self {
        Self {
            align: Align::Right,
            ..self
        }
    }

    pub const fn min_width(self, min_width: u16) -> Self {
        Self { min_width, ..self }
    }
}

/// A cell, colored when stdout gets colors.
pub fn cell(text: impl Into<String>, color: Option<Color>) -> Cell {
    let cell = Cell::new(text.into());
    match color {
        Some(color) if style::stdout_colored() => cell.fg(color),
        _ => cell,
    }
}

pub fn print(columns: &[Column], rows: Vec<Vec<Cell>>) {
    let mut table = Table::new();
    table
        .load_style(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic);
    let colored = style::stdout_colored();
    if colored && !table.is_tty() {
        // `--color always` into a pipe.
        table.enforce_styling();
    }

    table.set_header(columns.iter().map(|c| {
        let header = Cell::new(c.name);
        if colored {
            header.add_attribute(Attribute::Bold)
        } else {
            header
        }
    }));
    // A column with nothing in it (no notes this time, say) is just noise.
    let empty: Vec<bool> = (0..columns.len())
        .map(|i| {
            rows.iter()
                .all(|r| r.get(i).is_none_or(|c| c.content().is_empty()))
        })
        .collect();
    table.add_rows(rows);

    let width = table.width();
    for (i, column) in columns.iter().enumerate() {
        let Some(col) = table.column_mut(i) else {
            continue;
        };
        if column.align == Align::Right {
            col.set_cell_alignment(CellAlignment::Right);
        }
        if empty[i] || width.is_some_and(|w| w < column.min_width) {
            col.set_constraint(ColumnConstraint::Hidden);
        }
    }
    anstream::println!("{table}");
}
