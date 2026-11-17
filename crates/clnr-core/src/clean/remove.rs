//! Permanent removal of a directory tree.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use super::Journal;
use super::journal::{Entry, identity};
use crate::env::{Env, process_alive};
use crate::model::ArtifactId;

/// Prefix of the temporary name a directory gets while it is being deleted.
pub const TOMBSTONE_PREFIX: &str = ".clnr-del-";

/// Delete `path` permanently.
///
/// The directory is first renamed to a hidden sibling, which is atomic on one filesystem, so
/// tools never see a half-deleted `target/`, and recorded in `journal` so an interrupted delete
/// can be finished later. If the rename fails, it is deleted in place.
pub(super) fn remove(path: &Path, id: ArtifactId, journal: &Journal) -> io::Result<()> {
    let Some(target) = tombstone(path, id) else {
        return remove_tree(path);
    };
    // Best effort: without the record the delete still happens, it just can't be resumed.
    let recorded = journal.add(&target).is_ok();
    remove_tree(&target).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("{e} (partially deleted; leftover at {})", target.display()),
        )
    })?;
    if recorded {
        let _ = journal.remove(&target);
    }
    Ok(())
}

/// What [`resume`] did.
#[derive(Debug, Default)]
pub struct Resumed {
    pub removed: usize,
    pub errors: Vec<String>,
}

/// What to do with a journal entry.
enum Check {
    /// Still exactly the directory being deleted: finish it.
    Finish,
    /// Another clnr process that is still running owns it.
    Busy,
    /// Already gone: drop the entry.
    Gone,
    /// Not provably ours any more: leave it alone and drop the entry.
    Foreign(String),
}

/// Read-only: is this entry still exactly the directory that was recorded? Our name prefix,
/// the same device and inode, a real directory owned by us, outside `protect`, and not owned by
/// another running clnr.
fn check(entry: &Entry, env: &Env, protect: &[PathBuf]) -> Check {
    let path = &entry.path;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let Some(rest) = name.strip_prefix(TOMBSTONE_PREFIX) else {
        return Check::Foreign(format!(
            "{}: not a clnr tombstone; left alone",
            path.display()
        ));
    };
    let pid = rest.split('-').next().and_then(|p| p.parse::<u32>().ok());
    if pid.is_some_and(|pid| pid != std::process::id() && process_alive(pid)) {
        return Check::Busy;
    }
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Check::Gone,
        Err(e) => return Check::Foreign(format!("{}: {e}", path.display())),
    };
    let same = identity(&meta) == Some((entry.dev, entry.ino));
    let owned = env
        .euid()
        .is_none_or(|euid| owner(&meta).is_none_or(|uid| uid == euid));
    let protected = protect
        .iter()
        .any(|p| path.starts_with(p) || p.starts_with(path));
    if same && meta.is_dir() && owned && !protected {
        Check::Finish
    } else {
        Check::Foreign(format!(
            "{}: no longer the directory being deleted; left alone",
            path.display()
        ))
    }
}

/// Read-only: tombstones [`resume`] would finish.
pub fn resumable(journal: &Journal, env: &Env, protect: &[PathBuf]) -> Vec<PathBuf> {
    journal
        .pending()
        .into_iter()
        .filter(|e| matches!(check(e, env, protect), Check::Finish))
        .map(|e| e.path)
        .collect()
}

/// Finish deletes that a previous run left behind. Only entries [`check`] proves are still the
/// recorded directories are deleted; the rest are left alone and dropped from the journal.
pub fn resume(journal: &Journal, env: &Env, protect: &[PathBuf]) -> Resumed {
    let mut resumed = Resumed::default();
    for entry in journal.pending() {
        let path = &entry.path;
        match check(&entry, env, protect) {
            Check::Busy => {}
            Check::Gone => {
                let _ = journal.remove(path);
            }
            Check::Foreign(reason) => {
                resumed.errors.push(reason);
                let _ = journal.remove(path);
            }
            Check::Finish => match remove_tree(path) {
                Ok(()) => {
                    resumed.removed += 1;
                    let _ = journal.remove(path);
                }
                Err(e) => resumed.errors.push(format!("{}: {e}", path.display())),
            },
        }
    }
    resumed
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

fn tombstone(path: &Path, id: ArtifactId) -> Option<PathBuf> {
    let name = format!("{TOMBSTONE_PREFIX}{}-{id}", std::process::id());
    let target = path.parent()?.join(name);
    fs::rename(path, &target).ok()?;
    Some(target)
}

/// Remove a tree, subdirectories in parallel. Read-only directories (Go's module cache, Bazel
/// output bases) are made writable first. Symlinks are removed, never followed.
fn remove_tree(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() {
        return fs::remove_file(path);
    }
    make_writable(path, &meta)?;

    let mut subdirs = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            subdirs.push(entry.path());
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    subdirs.par_iter().try_for_each(|d| remove_tree(d))?;
    fs::remove_dir(path)
}

#[cfg(unix)]
fn make_writable(path: &Path, meta: &fs::Metadata) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = meta.permissions().mode();
    if mode & 0o700 != 0o700 {
        fs::set_permissions(path, fs::Permissions::from_mode(mode | 0o700))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn make_writable(path: &Path, meta: &fs::Metadata) -> io::Result<()> {
    let mut perms = meta.permissions();
    if perms.readonly() {
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}
