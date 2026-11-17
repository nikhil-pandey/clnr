//! Colors for plain-text output.
//!
//! Output goes through `anstream`, which drops colors when the stream isn't a terminal, when
//! `NO_COLOR` is set, or with `--color never` (and forces them with `--color always` or
//! `CLICOLOR_FORCE`).

use std::sync::OnceLock;

use anstream::ColorChoice;
use anstyle::{AnsiColor, Style};

pub const HEADER: Style = Style::new().bold();
pub const DIM: Style = Style::new().dimmed();
pub const OK: Style = AnsiColor::Green.on_default().bold();
pub const WARN: Style = AnsiColor::Yellow.on_default();
pub const ERROR: Style = AnsiColor::Red.on_default().bold();

/// Apply `--color` to everything written through `anstream`.
pub fn init(choice: clap::ColorChoice) {
    let choice = match choice {
        clap::ColorChoice::Auto => ColorChoice::Auto,
        clap::ColorChoice::Always => ColorChoice::Always,
        clap::ColorChoice::Never => ColorChoice::Never,
    };
    choice.write_global();
}

/// Will stdout get colors? Decided once, after [`init`].
pub fn stdout_colored() -> bool {
    static COLORED: OnceLock<bool> = OnceLock::new();
    *COLORED.get_or_init(|| anstream::AutoStream::choice(&std::io::stdout()) != ColorChoice::Never)
}

/// `warning: <message>` on stderr.
pub fn warn(message: impl std::fmt::Display) {
    anstream::eprintln!("{WARN}warning:{WARN:#} {message}");
}

/// `error: <message>` on stderr.
pub fn error(message: impl std::fmt::Display) {
    anstream::eprintln!("{ERROR}error:{ERROR:#} {message}");
}
