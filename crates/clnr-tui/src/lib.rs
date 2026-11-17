//! Interactive terminal UI for clnr.
//!
//! Elm-style: [`state`] is plain data, [`update`] turns messages into state changes plus
//! effects, [`ui`] renders state, and [`runtime`] runs the loop and the side effects.

#![forbid(unsafe_code)]

mod runtime;
mod state;
mod theme;
mod ui;
mod update;

use std::io;
use std::sync::Arc;

use clnr_core::scan::ScanOptions;
use clnr_core::{Env, Registry};
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

/// What the TUI works with, supplied by the binary.
pub struct Context {
    pub registry: Arc<Registry>,
    pub env: Env,
    pub options: ScanOptions,
    /// Capture the mouse (wheel and clicks). Off keeps the terminal's own text selection.
    pub mouse: bool,
    /// ASCII symbols instead of Unicode ones.
    pub ascii: bool,
}

pub use theme::prefers_ascii;

/// Run the TUI until the user quits. The terminal is restored afterwards, including on panic.
pub fn run(ctx: Context) -> io::Result<()> {
    ratatui::run(|terminal| {
        let _mouse = ctx.mouse.then(MouseCapture::enable).transpose()?;
        runtime::Runtime::new(ctx).run(terminal)
    })
}

/// Mouse capture for as long as this lives. Dropped on return and while unwinding from a panic,
/// so the terminal never keeps sending mouse sequences to the shell.
struct MouseCapture;

impl MouseCapture {
    fn enable() -> io::Result<Self> {
        execute!(io::stdout(), EnableMouseCapture)?;
        Ok(Self)
    }
}

impl Drop for MouseCapture {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
}
