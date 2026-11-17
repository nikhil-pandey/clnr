//! Product defaults and wiring: which config applies, what a run scans, and with which
//! providers.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use clnr_core::config::{self, Config, Settings};
use clnr_core::scan::{Scan, ScanOptions, ScanReport};
use clnr_core::{Env, Registry};

use crate::args::Cli;

/// Searched for projects when neither `--root` nor the config names any.
const DEFAULT_ROOTS: &[&str] = &["~/code", "/workspace"];

pub struct App {
    pub env: Env,
    pub registry: Arc<Registry>,
    pub options: ScanOptions,
    /// The config file in effect, if any.
    pub config: Option<PathBuf>,
}

impl App {
    pub fn new(cli: &Cli) -> Result<Self> {
        let env =
            Env::from_process().ok_or_else(|| anyhow!("could not determine the home directory"))?;
        let (config, settings) = load_settings(cli, &env)?;

        let mut registry = Registry::new(settings.providers);
        if !cli.provider.is_empty() {
            registry.retain_ids(&cli.provider)?;
        }

        let roots = if !cli.roots.is_empty() {
            cli.roots.clone()
        } else if let Some(roots) = settings.roots {
            roots
        } else {
            DEFAULT_ROOTS.iter().map(|r| env.expand(r)).collect()
        };

        Ok(Self {
            env,
            registry: Arc::new(registry),
            options: ScanOptions {
                roots,
                exclude: settings.exclude,
                protect: settings.protect,
            },
            config,
        })
    }

    pub fn into_tui_context(self, cli: &Cli) -> clnr_tui::Context {
        clnr_tui::Context {
            ascii: cli.ascii || clnr_tui::prefers_ascii(&self.env),
            mouse: !cli.no_mouse,
            registry: self.registry,
            env: self.env,
            options: self.options,
        }
    }

    /// Scan and wait for the result.
    pub fn scan(&self) -> ScanReport {
        Scan::start(Arc::clone(&self.registry), &self.env, &self.options).wait()
    }
}

/// The config file to use: `--config`, else the default location if it exists.
/// `Ok(None)` means no file (defaults apply).
pub fn config_path(cli: &Cli, env: &Env) -> Result<Option<PathBuf>> {
    if cli.no_config {
        return Ok(None);
    }
    match &cli.config {
        Some(path) if !path.exists() => bail!("config file {} does not exist", path.display()),
        Some(path) => Ok(Some(path.clone())),
        None => Ok(Some(config::default_path(env)).filter(|p| p.exists())),
    }
}

/// Load and check the config (or defaults), with the built-in providers.
pub fn load_settings(cli: &Cli, env: &Env) -> Result<(Option<PathBuf>, Settings)> {
    let path = config_path(cli, env)?;
    let config = match &path {
        Some(p) => Config::load(p)?,
        None => Config::default(),
    };
    let source = path.as_deref().unwrap_or(Path::new("<defaults>"));
    let settings = config.resolve(source, env, clnr_providers::builtin())?;
    Ok((path, settings))
}
