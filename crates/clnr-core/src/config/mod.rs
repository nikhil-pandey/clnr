//! The user's config file: roots, excludes, protected paths, provider overrides, and
//! providers defined in TOML.
//!
//! Loading is two steps: [`Config::load`] parses the file, then [`Config::resolve`] checks it
//! against the environment and the built-in providers and returns ready-to-use [`Settings`].
//! Every problem is reported at once, so one run of `clnr config check` shows them all.

mod declared;
mod overrides;

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::env::Env;
use crate::model::Safety;
use crate::provider::Provider;
use crate::scan::Excludes;

/// A commented starting point, written by `clnr config init`.
pub const TEMPLATE: &str = include_str!("template.toml");

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Replace the default roots.
    pub roots: Option<Vec<String>>,
    /// Glob patterns (or plain paths) the walk skips.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Paths never deleted, nor anything inside or containing them.
    #[serde(default)]
    pub protect: Vec<String>,
    /// Overrides for providers, by id.
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderOverride>,
    /// Providers defined here.
    #[serde(default, rename = "provider")]
    pub custom: Vec<CustomProvider>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderOverride {
    pub enabled: Option<bool>,
    /// By rule id (`registry-cache`, `node_modules`).
    #[serde(default)]
    pub rules: BTreeMap<String, RuleOverride>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleOverride {
    pub enabled: Option<bool>,
    /// Can only make a rule stricter.
    pub safety: Option<Safety>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomProvider {
    pub id: String,
    pub name: Option<String>,
    /// Executables that show the tool is installed.
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub global: Vec<CustomGlobal>,
    #[serde(default)]
    pub project: Vec<CustomRule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomGlobal {
    pub id: Option<String>,
    /// Absolute, or starting with `~`.
    pub path: String,
    pub label: Option<String>,
    pub safety: Safety,
    /// Required: at least one of these must exist inside for the location to count.
    pub verify: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomRule {
    pub id: Option<String>,
    pub dir: String,
    pub marker: MarkerConfig,
    #[serde(default)]
    pub verify: Vec<String>,
    pub safety: Safety,
}

/// `{ file = "mix.exs" }`, `{ any_file = [...] }`, `{ suffix = [".csproj"] }`, or `"always"`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerConfig {
    File(String),
    AnyFile(Vec<String>),
    Suffix(Vec<String>),
    Always,
}

/// A checked config, ready to use.
pub struct Settings {
    /// `None` means the defaults.
    pub roots: Option<Vec<PathBuf>>,
    pub exclude: Excludes,
    pub protect: Vec<PathBuf>,
    /// Built-ins with overrides applied, then the custom providers.
    pub providers: Vec<Box<dyn Provider>>,
}

#[derive(Debug)]
pub enum ConfigError {
    Read(PathBuf, io::Error),
    Parse(PathBuf, toml::de::Error),
    Invalid(PathBuf, Vec<String>),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(path, e) => write!(f, "{}: {e}", path.display()),
            Self::Parse(path, e) => write!(f, "{}: {e}", path.display()),
            Self::Invalid(path, problems) => {
                write!(f, "{}:", path.display())?;
                for p in problems {
                    write!(f, "\n  - {p}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// `$CLNR_CONFIG`, else `$XDG_CONFIG_HOME/clnr/config.toml` (also on macOS).
pub fn default_path(env: &Env) -> PathBuf {
    env.path_var("CLNR_CONFIG")
        .unwrap_or_else(|| env.config_dir().join("clnr/config.toml"))
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text =
            std::fs::read_to_string(path).map_err(|e| ConfigError::Read(path.to_owned(), e))?;
        toml::from_str(&text).map_err(|e| ConfigError::Parse(path.to_owned(), e))
    }

    /// Check everything and build [`Settings`]. `path` is only used in messages and as the
    /// source of custom provider locations.
    pub fn resolve(
        self,
        path: &Path,
        env: &Env,
        builtin: Vec<Box<dyn Provider>>,
    ) -> Result<Settings, ConfigError> {
        let mut problems = Vec::new();

        let roots = self.roots.as_ref().map(|roots| {
            roots
                .iter()
                .filter_map(|r| absolute(env, r, "roots", &mut problems))
                .collect()
        });
        let protect = self
            .protect
            .iter()
            .filter_map(|p| absolute(env, p, "protect", &mut problems))
            .collect();
        let patterns: Vec<String> = self
            .exclude
            .iter()
            .map(|p| env.expand(p).to_string_lossy().into_owned())
            .collect();
        let exclude = Excludes::new(&patterns).unwrap_or_else(|e| {
            problems.push(format!("exclude: {e}"));
            Excludes::default()
        });

        let builtin_ids: HashSet<&str> = builtin.iter().map(|p| p.id()).collect();
        let custom: Vec<Box<dyn Provider>> = self
            .custom
            .into_iter()
            .enumerate()
            .filter_map(|(i, c)| declared::build(c, i, path, env, &builtin_ids, &mut problems))
            .collect();
        let mut ids: HashSet<&str> = builtin_ids.clone();
        for p in &custom {
            if !ids.insert(p.id()) {
                problems.push(format!("provider `{}` is defined twice", p.id()));
            }
        }

        let mut providers = Vec::new();
        let mut overrides = self.providers;
        for provider in builtin.into_iter().chain(custom) {
            match overrides.remove(provider.id()) {
                Some(ov) => {
                    if let Some(p) = overrides::apply(provider, &ov, env, &mut problems) {
                        providers.push(p);
                    }
                }
                None => providers.push(provider),
            }
        }
        for id in overrides.keys() {
            problems.push(format!("providers.{id}: no such provider"));
        }

        if problems.is_empty() {
            Ok(Settings {
                roots,
                exclude,
                protect,
                providers,
            })
        } else {
            Err(ConfigError::Invalid(path.to_owned(), problems))
        }
    }
}

/// Expand `~` and require an absolute path.
fn absolute(env: &Env, raw: &str, field: &str, problems: &mut Vec<String>) -> Option<PathBuf> {
    let path = env.expand(raw);
    if path.is_absolute() {
        Some(path)
    } else {
        problems.push(format!("{field}: `{raw}` must be absolute or start with ~"));
        None
    }
}
