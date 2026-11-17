use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};

use clnr_core::{Artifact, Env, GlobalSpec, Marker, ProjectRule, Provider, Safety, Source};

use crate::common::{global, subdirs};

/// (rule, path relative to the tool's home dir, label, safety)
type Dirs = &'static [(&'static str, &'static str, &'static str, Safety)];

#[rustfmt::skip]
const CARGO_HOME_DIRS: Dirs = &[
    ("registry-cache", "registry/cache", "crate downloads", Safety::Redownload),
    ("registry-src",   "registry/src",   "crate sources",   Safety::Regenerable),
    ("registry-index", "registry/index", "registry index",  Safety::Redownload),
    ("git-db",         "git/db",         "git repos",       Safety::Redownload),
    ("git-checkouts",  "git/checkouts",  "git checkouts",   Safety::Regenerable),
    ("build-cache",    "build-cache",    "build cache",     Safety::Regenerable),
];

#[rustfmt::skip]
const RUSTUP_HOME_DIRS: Dirs = &[
    ("rustup-downloads", "downloads", "rustup downloads", Safety::Regenerable),
    ("rustup-tmp",       "tmp",       "rustup temp",      Safety::Regenerable),
];

const TARGET: &str = "target";
const SHARED_TARGET: &str = "shared-target";

/// Cargo writes these into every target dir it creates.
const TARGET_DIR_MARKERS: &[&str] = &["CACHEDIR.TAG", ".rustc_info.json"];

const PROJECT_RULES: &[ProjectRule] = &[ProjectRule {
    id: TARGET,
    dir: "target",
    marker: Marker::File("Cargo.toml"),
    verify: TARGET_DIR_MARKERS,
    safety: Safety::Regenerable,
}];

/// Cargo and rustup.
pub struct Cargo;

impl Cargo {
    fn cargo_home(env: &Env) -> (PathBuf, Source) {
        env.dir_or("CARGO_HOME", || env.home().join(".cargo"))
    }

    fn rustup_home(env: &Env) -> (PathBuf, Source) {
        env.dir_or("RUSTUP_HOME", || env.home().join(".rustup"))
    }

    /// A target dir shared by all projects: from the environment, or `build.target-dir` in
    /// `$CARGO_HOME/config.toml`.
    fn shared_target_dir(env: &Env) -> Option<(PathBuf, Source)> {
        for var in ["CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR"] {
            if let Some(path) = env.path_var(var) {
                return Some((path, Source::Env(var)));
            }
        }
        let (home, _) = Self::cargo_home(env);
        let config = home.join("config.toml");
        let table: toml::Table = fs::read_to_string(&config).ok()?.parse().ok()?;
        let raw = Path::new(table.get("build")?.get("target-dir")?.as_str()?);
        // Cargo resolves relative config paths against the parent of the `.cargo` directory.
        let path = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            home.parent()?.join(raw)
        };
        Some((path, Source::Config(config)))
    }
}

fn specs(dirs: Dirs, home: &Path, source: &Source) -> impl Iterator<Item = GlobalSpec> {
    dirs.iter().map(move |&(rule, rel, label, safety)| {
        global(rule, label, (home.join(rel), source.clone()), safety)
    })
}

impl Provider for Cargo {
    fn id(&self) -> &'static str {
        "cargo"
    }

    fn name(&self) -> &'static str {
        "Cargo"
    }

    fn tools(&self) -> &[&'static str] {
        &["cargo", "rustup"]
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        let (cargo_home, cargo_source) = Self::cargo_home(env);
        let mut out: Vec<GlobalSpec> = specs(CARGO_HOME_DIRS, &cargo_home, &cargo_source).collect();

        // `downloads` and `tmp` are generic names: only offer them inside a real rustup home.
        let (rustup_home, rustup_source) = Self::rustup_home(env);
        if rustup_home.join("toolchains").is_dir() {
            out.extend(specs(RUSTUP_HOME_DIRS, &rustup_home, &rustup_source));
        }

        if let Some((path, source)) = Self::shared_target_dir(env) {
            out.push(GlobalSpec {
                // Configurable to anything, so it must look like a target dir.
                verify: TARGET_DIR_MARKERS,
                ..global(
                    SHARED_TARGET,
                    "target (shared)",
                    (path, source),
                    Safety::Regenerable,
                )
            });
        }
        out
    }

    fn project_rules(&self) -> &[ProjectRule] {
        PROJECT_RULES
    }

    fn in_use(&self, artifact: &Artifact) -> Option<String> {
        if ![TARGET, SHARED_TARGET].contains(&artifact.rule) {
            return None;
        }
        artifact.paths().iter().find_map(|p| held_build_lock(p))
    }
}

/// Cargo holds an exclusive lock on `.cargo-lock` in each profile dir (`debug/`,
/// `<triple>/release/`, ...) while building. Probe them without waiting.
fn held_build_lock(target: &Path) -> Option<String> {
    subdirs(target)
        .flat_map(|profile| {
            let nested = subdirs(&profile).map(|d| d.join(".cargo-lock"));
            std::iter::once(profile.join(".cargo-lock")).chain(nested)
        })
        .find(|lock| {
            File::open(lock).is_ok_and(|f| matches!(f.try_lock(), Err(TryLockError::WouldBlock)))
        })
        .map(|lock| format!("cargo is building (lock held on {})", lock.display()))
}
