#![forbid(unsafe_code)]

mod app;
mod args;
mod cmd;
mod render;
mod style;
mod table;

use std::io::{self, IsTerminal};
use std::process::ExitCode;

use anyhow::{Result, anyhow};
use clap::{CommandFactory, Parser, error::ErrorKind};
use clnr_core::Env;

use crate::app::App;
use crate::args::{Cli, Command, ListArgs};

fn main() -> ExitCode {
    let cli = Cli::parse();
    style::init(cli.color);
    validate(&cli);
    match run(&cli) {
        Ok(code) => code,
        Err(e) => {
            style::error(format_args!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}

/// Usage errors clap can't express. Exits with clap's usage status.
fn validate(cli: &Cli) {
    if let Some(Command::Clean(args)) = &cli.command
        && !args.all
        && args.filter.is_empty()
        && cli.provider.is_empty()
    {
        Cli::command()
            .error(
                ErrorKind::MissingRequiredArgument,
                "`clean` needs a filter, --provider, or --all to clean everything",
            )
            .exit();
    }
}

fn run(cli: &Cli) -> Result<ExitCode> {
    // Config commands must work even when the config is broken, so they run before `App`.
    if let Some(Command::Config(command)) = &cli.command {
        let env =
            Env::from_process().ok_or_else(|| anyhow!("could not determine the home directory"))?;
        cmd::config::run(cli, command, &env)?;
        return Ok(ExitCode::SUCCESS);
    }
    let app = App::new(cli)?;
    match &cli.command {
        None if io::stdout().is_terminal() => {
            clnr_tui::run(app.into_tui_context(cli))?;
            Ok(ExitCode::SUCCESS)
        }
        // Piped: behave like `list`.
        None => cmd::list::run(&app, &ListArgs::default()).map(|()| ExitCode::SUCCESS),
        Some(Command::List(args)) => cmd::list::run(&app, args).map(|()| ExitCode::SUCCESS),
        Some(Command::Clean(args)) => cmd::clean::run(&app, args),
        Some(Command::Providers) => {
            cmd::providers::run(&app);
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Config(_)) => unreachable!("handled above"),
    }
}
