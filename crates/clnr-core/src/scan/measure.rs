use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use crate::model::Size;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement {
    pub size: Size,
    pub newest_mtime: Option<SystemTime>,
}

/// Measure a path like `du`: allocated blocks, each hardlinked inode counted once,
/// symlinks never followed. Returns `None` if the path does not exist.
///
/// Subdirectories are walked in parallel on the rayon pool. Unreadable entries add nothing.
pub fn measure(path: &Path) -> Option<Measurement> {
    let root = fs::symlink_metadata(path).ok()?;
    let shared = Shared {
        now: SystemTime::now(),
        total: Mutex::default(),
        seen: Mutex::default(),
    };
    let mut acc = Acc::default();
    acc.add(&root, shared.now);
    shared.merge(acc);
    if root.is_dir() {
        rayon::scope(|s| walk(path.to_path_buf(), s, &shared));
    }
    let acc = shared.total.into_inner().unwrap_or_else(|e| e.into_inner());
    Some(Measurement {
        size: Size {
            on_disk: acc.on_disk,
            apparent: acc.apparent,
            entries: acc.entries,
        },
        newest_mtime: acc.newest,
    })
}

/// Measure several paths as one artifact. Missing paths count as empty.
pub fn measure_paths(paths: &[PathBuf]) -> Measurement {
    paths.iter().filter_map(|p| measure(p)).fold(
        Measurement {
            size: Size::default(),
            newest_mtime: None,
        },
        |acc, m| Measurement {
            size: Size {
                on_disk: acc.size.on_disk + m.size.on_disk,
                apparent: acc.size.apparent + m.size.apparent,
                entries: acc.size.entries + m.size.entries,
            },
            newest_mtime: acc.newest_mtime.max(m.newest_mtime),
        },
    )
}

#[derive(Default)]
struct Acc {
    on_disk: u64,
    apparent: u64,
    entries: u64,
    newest: Option<SystemTime>,
}

impl Acc {
    /// Count one entry. Modification times after `now` are ignored: they say nothing about
    /// recent use (Bazel, for one, dates its install bases years ahead).
    fn add(&mut self, md: &Metadata, now: SystemTime) {
        self.on_disk += disk_bytes(md);
        self.apparent += md.len();
        self.entries += 1;
        if let Ok(m) = md.modified()
            && m <= now
        {
            self.newest = self.newest.max(Some(m));
        }
    }
}

struct Shared {
    now: SystemTime,
    total: Mutex<Acc>,
    /// (dev, ino) of hardlinked files already counted.
    seen: Mutex<HashSet<(u64, u64)>>,
}

impl Shared {
    fn merge(&self, acc: Acc) {
        let mut t = self.total.lock().unwrap_or_else(|e| e.into_inner());
        t.on_disk += acc.on_disk;
        t.apparent += acc.apparent;
        t.entries += acc.entries;
        t.newest = t.newest.max(acc.newest);
    }

    /// False if this is a hardlink to an inode we already counted.
    fn claim(&self, md: &Metadata) -> bool {
        match hardlink_key(md) {
            Some(key) => self
                .seen
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(key),
            None => true,
        }
    }
}

fn walk<'s>(dir: PathBuf, scope: &rayon::Scope<'s>, shared: &'s Shared) {
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let mut acc = Acc::default();
    for entry in entries.flatten() {
        // DirEntry::metadata does not follow symlinks.
        let Ok(md) = entry.metadata() else { continue };
        if !shared.claim(&md) {
            continue;
        }
        acc.add(&md, shared.now);
        if md.is_dir() {
            let path = entry.path();
            scope.spawn(move |s| walk(path, s, shared));
        }
    }
    shared.merge(acc);
}

#[cfg(unix)]
fn disk_bytes(md: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    md.blocks() * 512
}

#[cfg(not(unix))]
fn disk_bytes(md: &Metadata) -> u64 {
    md.len()
}

#[cfg(unix)]
fn hardlink_key(md: &Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (!md.is_dir() && md.nlink() > 1).then(|| (md.dev(), md.ino()))
}

#[cfg(not(unix))]
fn hardlink_key(_: &Metadata) -> Option<(u64, u64)> {
    None
}
