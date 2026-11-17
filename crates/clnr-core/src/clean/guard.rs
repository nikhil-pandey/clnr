//! Safety checks run before every delete. Deletes are permanent, so this is the part of clnr
//! that must never be wrong; every rule errs towards blocking.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::env::Env;
use crate::model::{Artifact, Location, Origin};
use crate::provider::{Provider, verified};
use crate::registry::Registry;
use crate::scan::ScanScope;

/// Why an artifact may not be deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocked(pub String);

impl fmt::Display for Blocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn blocked(reason: impl Into<String>) -> Result<(), Blocked> {
    Err(Blocked(reason.into()))
}

/// Decides whether an artifact may be deleted, against what a scan covered.
pub struct Guard<'a> {
    registry: &'a Registry,
    scope: &'a ScanScope,
    env: &'a Env,
    home: PathBuf,
    euid: Option<u32>,
}

impl<'a> Guard<'a> {
    pub fn new(registry: &'a Registry, scope: &'a ScanScope, env: &'a Env) -> Self {
        Self {
            registry,
            scope,
            env,
            home: env
                .home()
                .canonicalize()
                .unwrap_or_else(|_| env.home().to_path_buf()),
            euid: env.euid(),
        }
    }

    pub(super) fn provider(&self, artifact: &Artifact) -> Option<&dyn Provider> {
        self.registry.provider(artifact.provider)
    }

    pub(super) fn env(&self) -> &Env {
        self.env
    }

    /// Run every check. Call it when planning and again right before deleting: the filesystem
    /// may have changed in between.
    pub fn check(&self, artifact: &Artifact) -> Result<(), Blocked> {
        let Some(provider) = self.registry.provider(artifact.provider) else {
            return blocked(format!("unknown provider `{}`", artifact.provider));
        };
        match &artifact.location {
            Location::Paths { paths } if paths.is_empty() => return blocked("no paths"),
            Location::Paths { paths } => {
                for path in paths {
                    self.check_path(artifact, path)?;
                }
            }
            // Deleted by the owning tool by exact id, so nothing else can be hit.
            Location::Managed { .. } => {}
        }
        match provider.in_use(artifact) {
            Some(reason) => blocked(format!("in use: {reason}")),
            None => Ok(()),
        }
    }

    fn check_path(&self, artifact: &Artifact, path: &Path) -> Result<(), Blocked> {
        // 1. Exists, and is the real thing rather than a symlink to something else.
        let Ok(meta) = fs::symlink_metadata(path) else {
            return blocked("no longer exists");
        };
        if meta.file_type().is_symlink() {
            return blocked("is a symlink");
        }
        if path.canonicalize().ok().as_deref() != Some(path) {
            return blocked("path changed since the scan (a parent may now be a symlink)");
        }

        // 2. Never the home dir, a scan root, or anything containing them.
        if self.home.starts_with(path) {
            return blocked("contains the home directory");
        }
        if self.scope.roots.iter().any(|root| root.starts_with(path)) {
            return blocked("is or contains a scan root");
        }
        if self
            .scope
            .protect
            .iter()
            .any(|p| path.starts_with(p) || p.starts_with(path))
        {
            return blocked("protected by config");
        }

        // 3. Owned by us.
        if let Some(euid) = self.euid
            && owner(&meta).is_some_and(|uid| uid != euid)
        {
            return blocked("owned by another user");
        }

        // 4. Still what the scan found, inside what the scan covered.
        match &artifact.origin {
            Origin::Global => {
                let Some(global) = self.scope.globals.iter().find(|g| g.path == path) else {
                    return blocked("not a cache location resolved in this run");
                };
                if !verified(path, global.verify) {
                    return blocked("no longer looks like the cache it was");
                }
            }
            Origin::Project { .. } => {
                if !self.scope.roots.iter().any(|root| path.starts_with(root)) {
                    return blocked("outside the scan roots");
                }
                let Some(rule) = self.registry.project_rule(artifact.provider, artifact.rule)
                else {
                    return blocked(format!("unknown rule `{}`", artifact.rule));
                };
                if !rule.matches(path) {
                    return blocked(format!("no longer looks like a `{}` directory", rule.dir));
                }
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn owner(meta: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.uid())
}

#[cfg(not(unix))]
fn owner(_: &fs::Metadata) -> Option<u32> {
    None
}
