//! The one door to the outside world for providers: env vars, home, OS, PATH.
//! Tests build an [`Env`] by hand instead of reading the process environment.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Other,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Other
        }
    }
}

/// Where a resolved path came from. Shown by `clnr providers`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Env(&'static str),
    /// A tool's own config file.
    Config(PathBuf),
    Default,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Env(var) => write!(f, "${var}"),
            Self::Config(path) => write!(f, "{}", path.display()),
            Self::Default => f.write_str("default"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Env {
    home: PathBuf,
    os: Os,
    vars: HashMap<String, String>,
    euid: Option<u32>,
}

impl Env {
    /// Snapshot of the current process environment. `None` without a home dir.
    pub fn from_process() -> Option<Self> {
        let home = std::env::home_dir().filter(|h| h.is_absolute())?;
        Some(Self {
            home,
            os: Os::current(),
            vars: std::env::vars().collect(),
            euid: current_euid(),
        })
    }

    pub fn new(
        home: impl Into<PathBuf>,
        os: Os,
        vars: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        Self {
            home: home.into(),
            os,
            vars: vars.into_iter().collect(),
            euid: None,
        }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn os(&self) -> Os {
        self.os
    }

    /// Effective user id of the process, where the platform has one.
    pub fn euid(&self) -> Option<u32> {
        self.euid
    }

    pub fn is_root(&self) -> bool {
        self.euid == Some(0)
    }

    /// A non-empty env var.
    pub fn var(&self, name: &str) -> Option<&str> {
        self.vars
            .get(name)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// An env var holding an absolute path (`~/` is expanded). Relative values are ignored.
    pub fn path_var(&self, name: &str) -> Option<PathBuf> {
        let path = self.expand(self.var(name)?);
        path.is_absolute().then_some(path)
    }

    /// Resolve a directory: `$var` if set, else `default`.
    pub fn dir_or(
        &self,
        var: &'static str,
        default: impl FnOnce() -> PathBuf,
    ) -> (PathBuf, Source) {
        match self.path_var(var) {
            Some(p) => (p, Source::Env(var)),
            None => (default(), Source::Default),
        }
    }

    /// Expand a leading `~`.
    pub fn expand(&self, raw: &str) -> PathBuf {
        match raw.strip_prefix('~') {
            Some("") => self.home.clone(),
            Some(rest) if rest.starts_with('/') => self.home.join(&rest[1..]),
            _ => PathBuf::from(raw),
        }
    }

    /// Per-user cache dir: `$XDG_CACHE_HOME` or `~/.cache` on Linux, `~/Library/Caches` on macOS.
    pub fn cache_dir(&self) -> PathBuf {
        match self.os {
            Os::MacOs => self.home.join("Library/Caches"),
            _ => self
                .path_var("XDG_CACHE_HOME")
                .unwrap_or_else(|| self.home.join(".cache")),
        }
    }

    /// `$XDG_CACHE_HOME` or `~/.cache` on every OS, for tools that use it on macOS too.
    pub fn xdg_cache_dir(&self) -> PathBuf {
        self.path_var("XDG_CACHE_HOME")
            .unwrap_or_else(|| self.home.join(".cache"))
    }

    /// `$XDG_CONFIG_HOME` or `~/.config`, on every OS.
    pub fn config_dir(&self) -> PathBuf {
        self.path_var("XDG_CONFIG_HOME")
            .unwrap_or_else(|| self.home.join(".config"))
    }

    /// `$XDG_STATE_HOME` or `~/.local/state`, on every OS.
    pub fn state_dir(&self) -> PathBuf {
        self.path_var("XDG_STATE_HOME")
            .unwrap_or_else(|| self.home.join(".local/state"))
    }

    /// Per-user data dir: `$XDG_DATA_HOME` or `~/.local/share` on Linux, `~/Library/Application Support` on macOS.
    pub fn data_dir(&self) -> PathBuf {
        match self.os {
            Os::MacOs => self.home.join("Library/Application Support"),
            _ => self
                .path_var("XDG_DATA_HOME")
                .unwrap_or_else(|| self.home.join(".local/share")),
        }
    }

    /// Find an executable on `$PATH`.
    pub fn which(&self, program: &str) -> Option<PathBuf> {
        let path = self.var("PATH")?;
        std::env::split_paths(path)
            .map(|dir| dir.join(program))
            .find(|candidate| is_executable(candidate))
    }
}

/// Is a process with this id running? Errs towards `true` when it can't tell.
#[cfg(unix)]
pub fn process_alive(pid: u32) -> bool {
    use rustix::io::Errno;
    use rustix::process::{Pid, test_kill_process};
    let Some(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw) else {
        return false;
    };
    !matches!(test_kill_process(pid), Err(Errno::SRCH))
}

#[cfg(not(unix))]
pub fn process_alive(_pid: u32) -> bool {
    true
}

#[cfg(unix)]
fn current_euid() -> Option<u32> {
    Some(rustix::process::geteuid().as_raw())
}

#[cfg(not(unix))]
fn current_euid() -> Option<u32> {
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
