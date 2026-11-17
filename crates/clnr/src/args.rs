use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};
use clnr_core::{Filter, Safety};

/// Find and clean developer caches and build artifacts.
#[derive(Parser)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Only these providers (comma-separated ids, see `clnr providers`).
    #[arg(short, long, global = true, value_delimiter = ',')]
    pub provider: Vec<String>,

    /// Directories to search for projects [default: from config, else ~/code, /workspace].
    #[arg(short, long = "root", global = true, value_name = "PATH")]
    pub roots: Vec<PathBuf>,

    /// Config file [default: $CLNR_CONFIG, else ~/.config/clnr/config.toml].
    #[arg(long, global = true, value_name = "FILE", conflicts_with = "no_config")]
    pub config: Option<PathBuf>,

    /// Ignore the config file.
    #[arg(long, global = true)]
    pub no_config: bool,

    /// Don't capture the mouse in the TUI (keeps the terminal's own text selection).
    #[arg(long, global = true)]
    pub no_mouse: bool,

    /// Use ASCII instead of Unicode symbols in the TUI [default: from the locale].
    #[arg(long, global = true)]
    pub ascii: bool,

    /// When to use colors.
    #[arg(long, global = true, value_name = "WHEN", default_value_t = clap::ColorChoice::Auto)]
    pub color: clap::ColorChoice,
}

#[derive(Subcommand)]
pub enum Command {
    /// Print artifacts and their sizes.
    List(ListArgs),
    /// Permanently delete artifacts matching filters.
    Clean(CleanArgs),
    /// Show providers, the paths they resolve, and whether each tool was found.
    Providers,
    /// Manage the config file.
    #[command(subcommand)]
    Config(ConfigCommand),
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Print the config file's path.
    Path,
    /// Write a commented starting config (never overwrites).
    Init,
    /// Check the config file for problems.
    Check,
    /// Print the settings in effect.
    Show,
}

#[derive(Args, Default)]
pub struct ListArgs {
    #[command(flatten)]
    pub filter: FilterArgs,

    /// Print JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct CleanArgs {
    #[command(flatten)]
    pub filter: FilterArgs,

    /// Everything found (still not `caution` items unless `--safety` includes them; still asks).
    #[arg(long)]
    pub all: bool,

    /// Show what would be deleted, then stop.
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Don't ask for confirmation.
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Args, Default)]
pub struct FilterArgs {
    /// Only artifacts at least this big, e.g. 100MB.
    #[arg(long, value_name = "SIZE", value_parser = clnr_core::human::parse_size)]
    pub min_size: Option<u64>,

    /// Only artifacts with nothing modified for this long, e.g. 30d (units: s, m, h, d, w).
    #[arg(long, value_name = "AGE", value_parser = parse_age)]
    pub older_than: Option<Duration>,

    /// Only these safety classes (comma-separated).
    #[arg(long, value_enum, value_delimiter = ',')]
    pub safety: Vec<SafetyArg>,

    /// Only artifacts whose project no longer exists (e.g. Bazel output bases).
    #[arg(long)]
    pub orphans: bool,

    /// Only artifacts whose provider, name, or location contains this text.
    #[arg(long = "match", value_name = "TEXT")]
    pub text: Option<String>,
}

impl FilterArgs {
    pub fn is_empty(&self) -> bool {
        self.min_size.is_none()
            && self.older_than.is_none()
            && self.safety.is_empty()
            && !self.orphans
            && self.text.is_none()
    }

    /// Build a [`Filter`], using `default_safety` when no `--safety` was given.
    pub fn to_filter(&self, default_safety: &[Safety]) -> Filter {
        let safety = if self.safety.is_empty() {
            default_safety.to_vec()
        } else {
            self.safety.iter().copied().map(Safety::from).collect()
        };
        Filter {
            min_size: self.min_size,
            older_than: self.older_than,
            safety,
            orphans: self.orphans,
            text: self.text.clone(),
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SafetyArg {
    Regenerable,
    Redownload,
    Caution,
}

impl From<SafetyArg> for Safety {
    fn from(arg: SafetyArg) -> Self {
        match arg {
            SafetyArg::Regenerable => Self::Regenerable,
            SafetyArg::Redownload => Self::Redownload,
            SafetyArg::Caution => Self::Caution,
        }
    }
}

fn parse_age(s: &str) -> Result<Duration, String> {
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: u64 = num
        .parse()
        .map_err(|_| format!("expected a number followed by a unit, got `{s}`"))?;
    let secs = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        "w" => 7 * 86_400,
        _ => return Err(format!("unknown unit `{unit}` (use s, m, h, d, or w)")),
    };
    Ok(Duration::from_secs(n * secs))
}
