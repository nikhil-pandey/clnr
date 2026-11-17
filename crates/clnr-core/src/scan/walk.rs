use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use ignore::{DirEntry, WalkBuilder, WalkState};

use super::Excludes;
use crate::model::{Artifact, Location, Origin};
use crate::provider::{ProjectRule, Provider};
use crate::registry::Registry;

/// Version-control metadata: never interesting, often huge.
const SKIP_DIRS: &[&str] = &[".git", ".hg", ".jj", ".svn"];

pub(super) struct Walk<'a> {
    pub registry: &'a Registry,
    /// Canonical, non-overlapping roots.
    pub roots: &'a [PathBuf],
    /// Directories not to descend into (global artifacts, measured separately).
    pub skip: &'a [PathBuf],
    pub exclude: &'a Excludes,
    pub cancel: &'a AtomicBool,
    pub found: &'a (dyn Fn(Artifact) + Sync),
    pub error: &'a (dyn Fn(String) + Sync),
}

/// Walk the roots in parallel, reporting every directory that matches a project rule.
///
/// Ignore files are not honored (build output is almost always gitignored), symlinks are not
/// followed, each root stays on its own filesystem, and matched directories are not descended
/// into.
pub(super) fn walk(w: &Walk<'_>) {
    let Some((first, rest)) = w.roots.split_first() else {
        return;
    };
    let mut builder = WalkBuilder::new(first);
    for root in rest {
        builder.add(root);
    }
    let skip = w.skip.to_vec();
    let exclude = w.exclude.clone();
    builder
        .standard_filters(false)
        .follow_links(false)
        .same_file_system(true)
        .filter_entry(move |e| !is_skipped(e, &skip) && !exclude.is_match(e.path()));

    builder.build_parallel().run(|| {
        Box::new(move |entry| match entry {
            Err(e) => {
                (w.error)(e.to_string());
                WalkState::Continue
            }
            Ok(_) if w.cancel.load(Ordering::Relaxed) => WalkState::Quit,
            Ok(entry) => visit(w, &entry),
        })
    });
}

fn is_skipped(entry: &DirEntry, skip: &[PathBuf]) -> bool {
    let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
    is_dir
        && (entry
            .file_name()
            .to_str()
            .is_some_and(|n| SKIP_DIRS.contains(&n))
            || skip.iter().any(|s| s == entry.path()))
}

fn visit(w: &Walk<'_>, entry: &DirEntry) -> WalkState {
    let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
    let Some(name) = entry.file_name().to_str() else {
        return WalkState::Continue;
    };
    if entry.depth() == 0 || !is_dir {
        return WalkState::Continue;
    }
    let path = entry.path();
    match w
        .registry
        .rules_for_dir(name)
        .find(|(_, rule)| rule.matches(path))
    {
        Some((provider, rule)) => {
            (w.found)(project_artifact(provider, rule, path));
            WalkState::Skip
        }
        None => WalkState::Continue,
    }
}

fn project_artifact(provider: &dyn Provider, rule: &ProjectRule, path: &Path) -> Artifact {
    // `rule.matches` guarantees a parent.
    let root = path.parent().unwrap_or(path).to_path_buf();
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Artifact::new(
        provider.id(),
        rule.id,
        rule.dir,
        Origin::Project { root, name },
        Location::path(path),
        rule.safety,
    )
}
