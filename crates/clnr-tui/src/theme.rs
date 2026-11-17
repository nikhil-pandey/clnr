//! Colors and symbols. Plain ANSI colors so the terminal's palette (light or dark) applies;
//! with `NO_COLOR`, only text attributes. ASCII symbols for terminals or fonts without the
//! Unicode ones.

use clnr_core::Env;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::widgets::Block;

pub struct Theme {
    pub accent: Style,
    pub dim: Style,
    pub warn: Style,
    pub error: Style,
    pub marked: Style,
    pub cursor: Style,
    pub symbols: &'static Symbols,
}

pub struct Symbols {
    pub spinner: &'static [&'static str],
    /// Selected row / fully selected project.
    pub marked: &'static str,
    /// Partly selected project.
    pub partial: &'static str,
    /// Current sidebar entry.
    pub pointer: &'static str,
    /// Text input caret.
    pub caret: &'static str,
    pub ellipsis: &'static str,
    /// Between items in titles and hints.
    pub sep: &'static str,
    pub up: &'static str,
    pub down: &'static str,
    pub border: border::Set<'static>,
}

const UNICODE: Symbols = Symbols {
    spinner: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
    marked: "■",
    partial: "▪",
    pointer: "▶",
    caret: "█",
    ellipsis: "…",
    sep: " · ",
    up: "▲",
    down: "▼",
    border: border::PLAIN,
};

const ASCII: Symbols = Symbols {
    spinner: &["|", "/", "-", "\\"],
    marked: "*",
    partial: "+",
    pointer: ">",
    caret: "_",
    ellipsis: "...",
    sep: " | ",
    up: "^",
    down: "v",
    border: border::Set {
        top_left: "+",
        top_right: "+",
        bottom_left: "+",
        bottom_right: "+",
        vertical_left: "|",
        vertical_right: "|",
        horizontal_top: "-",
        horizontal_bottom: "-",
    },
};

impl Theme {
    pub fn new(no_color: bool, ascii: bool) -> Self {
        let symbols = if ascii { &ASCII } else { &UNICODE };
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let cursor = Style::new().add_modifier(Modifier::REVERSED);
        if no_color {
            return Self {
                accent: bold,
                dim: Style::new().add_modifier(Modifier::DIM),
                warn: bold,
                error: bold,
                marked: bold,
                cursor,
                symbols,
            };
        }
        Self {
            accent: Style::new().fg(Color::Cyan),
            dim: Style::new().fg(Color::DarkGray),
            warn: Style::new().fg(Color::Yellow),
            error: Style::new().fg(Color::Red),
            marked: bold.fg(Color::Green),
            cursor,
            symbols,
        }
    }

    /// A bordered block in this theme's border style.
    pub fn block(&self) -> Block<'static> {
        Block::bordered().border_set(self.symbols.border)
    }
}

/// Does the locale say the terminal can't show Unicode? The first of `LC_ALL`, `LC_CTYPE`, and
/// `LANG` that is set decides; with none set, Unicode is assumed.
pub fn prefers_ascii(env: &Env) -> bool {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|var| env.var(var))
        .is_some_and(|locale| {
            let locale = locale.to_ascii_lowercase();
            !(locale.contains("utf-8") || locale.contains("utf8"))
        })
}
