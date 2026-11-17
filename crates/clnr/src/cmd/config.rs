use std::fs;

use anyhow::{Context, Result, bail};
use clnr_core::{Env, config, human};

use crate::app;
use crate::args::{Cli, ConfigCommand};

pub fn run(cli: &Cli, command: &ConfigCommand, env: &Env) -> Result<()> {
    match command {
        ConfigCommand::Path => {
            let path = cli
                .config
                .clone()
                .unwrap_or_else(|| config::default_path(env));
            let note = if path.exists() {
                ""
            } else {
                " (not created yet: `clnr config init`)"
            };
            anstream::println!("{}{note}", path.display());
            Ok(())
        }
        ConfigCommand::Init => {
            let path = cli
                .config
                .clone()
                .unwrap_or_else(|| config::default_path(env));
            if path.exists() {
                bail!("{} already exists", path.display());
            }
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            }
            fs::write(&path, config::TEMPLATE)
                .with_context(|| format!("writing {}", path.display()))?;
            anstream::println!("Wrote {}", path.display());
            Ok(())
        }
        ConfigCommand::Check => {
            let (path, settings) = app::load_settings(cli, env)?;
            match path {
                Some(path) => anstream::println!("{}: ok", path.display()),
                None => anstream::println!("No config file; using defaults."),
            }
            anstream::println!(
                "{} enabled",
                human::count(settings.providers.len(), "provider")
            );
            Ok(())
        }
        ConfigCommand::Show => show(cli, env),
    }
}

fn show(cli: &Cli, env: &Env) -> Result<()> {
    let app = app::App::new(cli)?;
    let home = env.home();
    let paths = |list: &[std::path::PathBuf]| -> String {
        if list.is_empty() {
            "(none)".to_owned()
        } else {
            list.iter()
                .map(|p| human::path(p, home))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let config = app
        .config
        .as_deref()
        .map_or_else(|| "(none; defaults)".to_owned(), |p| human::path(p, home));
    let exclude = app.options.exclude.patterns();
    anstream::println!("config:    {config}");
    anstream::println!("roots:     {}", paths(&app.options.roots));
    anstream::println!(
        "exclude:   {}",
        if exclude.is_empty() {
            "(none)".to_owned()
        } else {
            exclude.join(", ")
        }
    );
    anstream::println!("protect:   {}", paths(&app.options.protect));
    let ids: Vec<&str> = app.registry.providers().iter().map(|p| p.id()).collect();
    anstream::println!("providers: {}", ids.join(", "));
    Ok(())
}
