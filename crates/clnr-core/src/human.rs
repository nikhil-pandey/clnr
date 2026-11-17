//! Human-readable formatting of core values, shared by every front end.

use std::path::Path;
use std::time::SystemTime;

use bytesize::ByteSize;

use crate::model::{Artifact, Location};

/// `18.2G`, compatible with `sort -h`.
pub fn size(bytes: u64) -> String {
    ByteSize(bytes).display().iec_short().to_string()
}

/// Parse a size such as `100MB`, `1.5GiB`, or `500K`.
pub fn parse_size(s: &str) -> Result<u64, String> {
    s.parse::<ByteSize>().map(|b| b.as_u64())
}

/// Compact age since `t`: `42s`, `3m`, `5h`, `12d`, `8mo`, `2y`.
pub fn age(t: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(t)
        .map_or(0, |d| d.as_secs());
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s if s < 60 * 86_400 => format!("{}d", s / 86_400),
        s if s < 365 * 86_400 => format!("{}mo", s / (30 * 86_400)),
        s => format!("{}y", s / (365 * 86_400)),
    }
}

/// A path with the home dir shortened to `~`.
pub fn path(p: &Path, home: &Path) -> String {
    match p.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

/// `41d ago`; `just now` under a minute.
pub fn ago(t: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(t)
        .map_or(0, |d| d.as_secs());
    if secs < 60 {
        "just now".to_owned()
    } else {
        format!("{} ago", age(t))
    }
}

/// Where an artifact actually is: its first path (shortened), or a managed object's name.
pub fn location(artifact: &Artifact, home: &Path) -> String {
    match &artifact.location {
        Location::Paths { paths } => paths.first().map(|p| path(p, home)).unwrap_or_default(),
        Location::Managed { name, .. } => name.clone(),
    }
}

/// What a user recognizes an artifact by: the project it belongs to (where it was found, or
/// its owner, such as a Bazel output base's workspace), else its display name, else its
/// location.
pub fn place(artifact: &Artifact, home: &Path) -> String {
    match (artifact.project(), &artifact.display) {
        (Some(project), _) => path(project, home),
        (None, Some(display)) => display.clone(),
        (None, None) => location(artifact, home),
    }
}

/// One artifact in a sentence: `cargo target ~/code/foo`, `docker image postgres:17`.
pub fn describe(artifact: &Artifact, home: &Path) -> String {
    format!(
        "{} {} {}",
        artifact.provider,
        artifact.label,
        place(artifact, home)
    )
}

/// `1 item`, `3 items`.
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}
