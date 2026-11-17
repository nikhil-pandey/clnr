use std::fs;
use std::path::{Path, PathBuf};

use crate::env::{Env, Source};
use crate::model::{Artifact, Safety};

/// Whether a provider's tool is around.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detection {
    /// The tool is on `PATH`.
    Installed(PathBuf),
    /// The tool is not on `PATH`, but its data is on disk.
    DataOnly,
    /// Neither the tool nor its data was found.
    Missing,
}

/// A fixed per-user location a provider knows about.
#[derive(Clone, Debug)]
pub struct GlobalSpec {
    /// Unique within the provider (`registry-src`).
    pub rule: &'static str,
    /// Human label (`registry/src`).
    pub label: String,
    pub path: PathBuf,
    pub safety: Safety,
    pub source: Source,
    /// At least one of these must exist inside for the location to count. Empty means no
    /// check; use it for locations that are not user-configurable to something arbitrary.
    pub verify: &'static [&'static str],
    /// The project this location belongs to, if known (a Bazel output base's workspace).
    /// Shown for grouping; a missing owner marks the artifact as orphaned.
    pub owner: Option<PathBuf>,
    /// Shown instead of the path when the path means nothing to people (hash-named
    /// directories): `bazel 9.3.2`.
    pub display: Option<String>,
}

/// A build-output directory found by walking roots: `dir` sitting next to `marker`.
#[derive(Clone, Debug)]
pub struct ProjectRule {
    /// Unique within the provider (`target`).
    pub id: &'static str,
    /// Exact directory name to look for.
    pub dir: &'static str,
    /// What must sit next to the directory.
    pub marker: Marker,
    /// At least one of these must exist inside the directory. Empty means no check.
    pub verify: &'static [&'static str],
    pub safety: Safety,
}

impl ProjectRule {
    /// Does `path` (a directory named [`Self::dir`]) satisfy the marker and verify checks?
    pub fn matches(&self, path: &Path) -> bool {
        let Some(parent) = path.parent() else {
            return false;
        };
        path.file_name().is_some_and(|n| n == self.dir)
            && self.marker.is_present_in(parent)
            && verified(path, self.verify)
    }
}

/// What must sit next to a [`ProjectRule`] directory, in its parent.
#[derive(Clone, Copy, Debug)]
pub enum Marker {
    /// A file with this exact name.
    File(&'static str),
    /// A file with any of these names.
    AnyFile(&'static [&'static str]),
    /// A file whose name ends with any of these (`.csproj`, `.tf`).
    AnySuffix(&'static [&'static str]),
    /// Nothing: the directory is identified by its name and `verify` alone.
    Always,
}

impl Marker {
    pub fn is_present_in(&self, dir: &Path) -> bool {
        match self {
            Self::File(name) => dir.join(name).is_file(),
            Self::AnyFile(names) => names.iter().any(|n| dir.join(n).is_file()),
            Self::AnySuffix(suffixes) => {
                fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
                    e.file_type().is_ok_and(|t| t.is_file())
                        && e.file_name()
                            .to_str()
                            .is_some_and(|n| suffixes.iter().any(|s| n.ends_with(s)))
                })
            }
            Self::Always => true,
        }
    }
}

/// True if `verify` is empty or at least one of its entries exists inside `dir`.
pub fn verified(dir: &Path, verify: &[&str]) -> bool {
    verify.is_empty()
        || verify
            .iter()
            .any(|v| dir.join(v).symlink_metadata().is_ok())
}

/// Knows one tool family: where its caches live and what is safe to delete.
///
/// Implementations live in `clnr-providers` and only describe things: walking, measuring,
/// and deleting are done by the core. Everything external goes through [`Env`] so providers
/// stay deterministic.
pub trait Provider: Send + Sync {
    /// Stable id used on the command line (`cargo`).
    fn id(&self) -> &'static str;

    /// Display name (`Cargo`).
    fn name(&self) -> &'static str;

    /// Executables that indicate the tool is installed.
    fn tools(&self) -> &[&'static str] {
        &[]
    }

    /// Default: installed if any of [`Self::tools`] is on `PATH`, else data-only if any global
    /// location exists.
    fn detect(&self, env: &Env) -> Detection {
        if let Some(path) = self.tools().iter().find_map(|t| env.which(t)) {
            Detection::Installed(path)
        } else if self.globals(env).iter().any(|g| g.path.exists()) {
            Detection::DataOnly
        } else {
            Detection::Missing
        }
    }

    /// Fixed locations. Paths that don't exist are dropped by the scanner.
    fn globals(&self, _env: &Env) -> Vec<GlobalSpec> {
        vec![]
    }

    /// Directories to look for while walking roots.
    fn project_rules(&self) -> &[ProjectRule] {
        &[]
    }

    /// Artifacts that are not directories found by path, e.g. Docker images. Runs on a
    /// background thread alongside the walk; must return [`Location::Managed`](crate::model::Location::Managed) artifacts.
    fn discover(&self, _env: &Env) -> Result<Vec<Artifact>, String> {
        Ok(vec![])
    }

    /// Delete a [`Location::Managed`](crate::model::Location::Managed) artifact through the owning tool, by exact id. Never
    /// called for paths: those are always removed by the core.
    fn delete(&self, _artifact: &Artifact, _env: &Env) -> Result<(), String> {
        Err(format!("{} cannot delete managed artifacts", self.id()))
    }

    /// Checked right before deletion. `Some(reason)` blocks the delete.
    fn in_use(&self, _artifact: &Artifact) -> Option<String> {
        None
    }
}
