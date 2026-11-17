//! Small helpers shared by provider modules.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use clnr_core::{GlobalSpec, Safety, Source};

/// A global location with no identity check. Set `verify` with struct update syntax when the
/// path is user-configurable: `GlobalSpec { verify: &["..."], ..global(...) }`.
pub(crate) fn global(
    rule: &'static str,
    label: &str,
    (path, source): (PathBuf, Source),
    safety: Safety,
) -> GlobalSpec {
    GlobalSpec {
        rule,
        label: label.to_owned(),
        path,
        safety,
        source,
        verify: &[],
        owner: None,
        display: None,
    }
}

/// A platform-default path.
pub(crate) fn default(path: PathBuf) -> (PathBuf, Source) {
    (path, Source::Default)
}

/// Immediate subdirectories of `dir` (not following symlinks). Empty if unreadable.
pub(crate) fn subdirs(dir: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
}

/// Run a tool and return its stdout. Stdin is closed; a failure returns trimmed stderr.
pub(crate) fn run(program: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run {}: {e}", program.display()))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}
