//! A record of directories being deleted, so a delete interrupted by a crash or kill can be
//! finished on the next run.
//!
//! Each line is `<device> <inode> <tombstone path>` (see [`super::TOMBSTONE_PREFIX`]), added
//! right after the rename and removed once the tombstone is gone. The device and inode let a
//! later run prove a directory is the same one, not something new at the same path.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::env::Env;

/// One recorded tombstone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub dev: u64,
    pub ino: u64,
}

pub struct Journal {
    path: PathBuf,
    /// Serializes writes from parallel deletes in this process.
    lock: Mutex<()>,
}

impl Journal {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    /// `$XDG_STATE_HOME/clnr/pending` (`~/.local/state/clnr/pending`).
    pub fn in_state_dir(env: &Env) -> Self {
        Self::new(env.state_dir().join("clnr/pending"))
    }

    /// Record a tombstone with its current identity. Errors if the identity can't be read
    /// (including on platforms without inode numbers): then the delete is not resumable.
    pub(super) fn add(&self, tombstone: &Path) -> io::Result<()> {
        let (dev, ino) = identity(&fs::symlink_metadata(tombstone)?)
            .ok_or_else(|| io::Error::other("no inode identity on this platform"))?;
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{dev} {ino} {}", tombstone.display())
    }

    pub(super) fn remove(&self, tombstone: &Path) -> io::Result<()> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let kept: Vec<Entry> = self
            .read()
            .into_iter()
            .filter(|e| e.path != tombstone)
            .collect();
        if kept.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        let text: String = kept
            .iter()
            .map(|e| format!("{} {} {}\n", e.dev, e.ino, e.path.display()))
            .collect();
        fs::write(&self.path, text)
    }

    /// Tombstones recorded and not yet confirmed gone. Malformed lines are skipped.
    pub fn pending(&self) -> Vec<Entry> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.read()
    }

    fn read(&self) -> Vec<Entry> {
        fs::read_to_string(&self.path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, ' ');
                Some(Entry {
                    dev: parts.next()?.parse().ok()?,
                    ino: parts.next()?.parse().ok()?,
                    path: PathBuf::from(parts.next()?),
                })
            })
            .collect()
    }
}

/// `(device, inode)`, where the platform has them.
#[cfg(unix)]
pub(super) fn identity(meta: &fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
pub(super) fn identity(_: &fs::Metadata) -> Option<(u64, u64)> {
    None
}
