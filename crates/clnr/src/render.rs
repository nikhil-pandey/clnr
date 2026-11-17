//! Terminal output shared by commands.

use clnr_core::{Artifact, Env, Safety, human};
use comfy_table::{Cell, Color};

use crate::style::{self, HEADER};
use crate::table::{self, Column};

/// What a user needs to decide: what it is, whose it is, how big, how stale, and anything that
/// should give pause. On narrow terminals TOOL hides first (WHAT mostly implies it), then
/// CHANGED.
const ARTIFACT_COLUMNS: &[Column] = &[
    Column::new("TOOL").min_width(80),
    Column::new("WHAT"),
    Column::new("WHERE"),
    Column::new("SIZE").right(),
    Column::new("CHANGED").right().min_width(56),
    Column::new("NOTE"),
];

/// The artifact table plus a total line.
pub fn artifacts<'a>(artifacts: impl IntoIterator<Item = &'a Artifact>, env: &Env) {
    let artifacts: Vec<&Artifact> = artifacts.into_iter().collect();
    let rows = artifacts.iter().map(|a| artifact_row(a, env)).collect();
    table::print(ARTIFACT_COLUMNS, rows);

    let total: u64 = artifacts.iter().map(|a| a.on_disk()).sum();
    anstream::println!(
        "\n{HEADER}{}, {} total{HEADER:#}",
        human::count(artifacts.len(), "item"),
        human::size(total)
    );
}

fn artifact_row(a: &Artifact, env: &Env) -> Vec<Cell> {
    vec![
        Cell::new(a.provider),
        Cell::new(&a.label),
        Cell::new(human::place(a, env.home())),
        Cell::new(a.size.map(|s| human::size(s.on_disk)).unwrap_or_default()),
        Cell::new(a.newest_mtime.map(human::ago).unwrap_or_default()),
        table::cell(note(a), Some(Color::Yellow)),
    ]
}

/// Only what should give pause; empty for the usual case.
fn note(a: &Artifact) -> String {
    let mut notes = Vec::new();
    if a.safety == Safety::Caution {
        notes.push("caution");
    }
    if a.orphaned {
        notes.push("orphan");
    }
    notes.join(", ")
}

/// Paths the scan could not read, summarized on stderr.
pub fn scan_errors(errors: &[String]) {
    const SHOWN: usize = 5;
    if errors.is_empty() {
        return;
    }
    style::warn(format_args!("{} paths could not be read:", errors.len()));
    for e in errors.iter().take(SHOWN) {
        anstream::eprintln!("  {e}");
    }
    if errors.len() > SHOWN {
        anstream::eprintln!("  ... and {} more", errors.len() - SHOWN);
    }
}
