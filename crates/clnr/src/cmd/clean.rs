use std::cmp::Reverse;
use std::collections::HashMap;
use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;
use std::time::SystemTime;

use anyhow::{Result, bail};
use clnr_core::clean::{self, CleanEvent, Guard, Journal, Plan};
use clnr_core::{Artifact, Safety, human};

use crate::app::App;
use crate::args::CleanArgs;
use crate::render;
use crate::style::{self, DIM, OK, WARN};

/// `Caution` items are only deleted when asked for by name.
const DEFAULT_SAFETY: &[Safety] = &[Safety::Regenerable, Safety::Redownload];

pub fn run(app: &App, args: &CleanArgs) -> Result<ExitCode> {
    if app.env.is_root() {
        bail!("refusing to delete as root");
    }

    let report = app.scan();
    render::scan_errors(&report.errors);

    let filter = args.filter.to_filter(DEFAULT_SAFETY);
    let now = SystemTime::now();
    let mut selected: Vec<Artifact> = report
        .artifacts
        .into_iter()
        .filter(|a| filter.matches(a, now))
        .collect();
    selected.sort_by_key(|a| Reverse(a.on_disk()));
    if selected.is_empty() {
        anstream::println!("Nothing matched.");
        return Ok(ExitCode::SUCCESS);
    }

    let guard = Guard::new(&app.registry, &report.scope, &app.env);
    let plan = Plan::new(&guard, selected);
    let allowed: Vec<&Artifact> = plan.allowed().collect();
    print_plan(&plan, &allowed, app);

    // Deletes interrupted in an earlier run are only finished along with a confirmed delete.
    let journal = Journal::in_state_dir(&app.env);
    let interrupted = clean::resumable(&journal, &app.env, &app.options.protect).len();
    let also = if interrupted > 0 {
        format!(
            " and finish {} interrupted earlier",
            human::count(interrupted, "delete")
        )
    } else {
        String::new()
    };

    if args.dry_run || allowed.is_empty() {
        if interrupted > 0 {
            anstream::println!(
                "\n{} interrupted earlier would also be finished.",
                human::count(interrupted, "delete")
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    let total: u64 = allowed.iter().map(|a| a.on_disk()).sum();
    if !args.yes
        && !confirm(&format!(
            "Permanently delete {} ({}){also}?",
            human::count(allowed.len(), "item"),
            human::size(total)
        ))?
    {
        anstream::println!("Cancelled.");
        return Ok(ExitCode::SUCCESS);
    }
    finish_interrupted(&journal, app);

    let by_id: HashMap<_, _> = allowed.iter().map(|a| (a.id, *a)).collect();
    let home = app.env.home();
    let summary = clean::run(&guard, &journal, &allowed, &|event| match event {
        CleanEvent::Removed { id, freed } => {
            anstream::println!(
                "{DIM}removed {} ({}){DIM:#}",
                human::describe(by_id[&id], home),
                human::size(freed)
            );
        }
        CleanEvent::Failed { id, error } => {
            style::error(format_args!(
                "{}: {error}",
                human::describe(by_id[&id], home)
            ));
        }
        CleanEvent::Blocked { id, reason } => {
            style::warn(format_args!(
                "skipped {}: {reason}",
                human::describe(by_id[&id], home)
            ));
        }
    });

    anstream::println!(
        "\n{OK}Freed {} from {}.{OK:#}",
        human::size(summary.freed),
        human::count(summary.removed, "item")
    );
    if summary.failed + summary.blocked > 0 {
        style::warn(format_args!(
            "{} failed, {} skipped.",
            summary.failed, summary.blocked
        ));
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// Finish deletes an earlier run was interrupted in (the user has confirmed a delete).
fn finish_interrupted(journal: &Journal, app: &App) {
    let resumed = clean::resume(journal, &app.env, &app.options.protect);
    if resumed.removed > 0 {
        anstream::println!(
            "finished {} interrupted earlier",
            human::count(resumed.removed, "delete")
        );
    }
    for e in &resumed.errors {
        style::warn(e);
    }
}

fn print_plan(plan: &Plan, allowed: &[&Artifact], app: &App) {
    if !allowed.is_empty() {
        render::artifacts(allowed.iter().copied(), &app.env);
    }
    let blocked: Vec<_> = plan.blocked().collect();
    if !blocked.is_empty() {
        let gap = if allowed.is_empty() { "" } else { "\n" };
        anstream::println!(
            "{gap}{WARN}Skipping {}:{WARN:#}",
            human::count(blocked.len(), "item")
        );
        for (a, reason) in blocked {
            anstream::println!("  {}: {reason}", human::describe(a, app.env.home()));
        }
    }
}

/// Ask on the terminal. Refuses when stdin is not a terminal: use `--yes` in scripts.
fn confirm(question: &str) -> Result<bool> {
    let stdin = io::stdin();
    if !stdin.is_terminal() {
        bail!("stdin is not a terminal; pass --yes to delete without confirmation");
    }
    print!("\n{question} [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    stdin.lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}
