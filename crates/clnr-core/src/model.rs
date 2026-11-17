use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// How costly it is to lose an artifact. Ordered from least to most careful.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Safety {
    /// Rebuilt by the next build (`target/`, `obj/`, DerivedData).
    Regenerable,
    /// Re-fetched from the network on next use (package caches).
    Redownload,
    /// May hold state the user cares about (`.venv/`, Docker volumes).
    Caution,
}

impl Safety {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Regenerable => "regenerable",
            Self::Redownload => "redownload",
            Self::Caution => "caution",
        }
    }

    /// One line explaining what deleting it costs.
    pub fn description(self) -> &'static str {
        match self {
            Self::Regenerable => "Rebuilt by the next build.",
            Self::Redownload => "Downloaded again on next use.",
            Self::Caution => "May hold state you care about, or be slow to get back.",
        }
    }
}

/// Where an artifact came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Origin {
    /// A fixed per-user location such as `~/.cargo/registry`.
    Global,
    /// Found next to a marker file inside a project.
    Project { root: PathBuf, name: String },
}

/// What an artifact consists of.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Location {
    /// Directories on disk, measured and deleted by the core.
    Paths { paths: Vec<PathBuf> },
    /// Objects owned by a tool (Docker images, containers), sized by the tool and deleted
    /// through it by exact id.
    Managed {
        ids: Vec<String>,
        /// What the objects are called (`postgres:17`, `19 untagged`).
        name: String,
    },
}

impl Location {
    pub fn path(path: impl Into<PathBuf>) -> Self {
        Self::Paths {
            paths: vec![path.into()],
        }
    }
}

/// Disk usage with `du` semantics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Size {
    /// Allocated bytes; each hardlinked inode counted once.
    pub on_disk: u64,
    /// Sum of file lengths.
    pub apparent: u64,
    /// Number of entries (files, dirs, links), including the root.
    pub entries: u64,
}

/// Stable identity for an artifact: provider + location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactId(u64);

impl ArtifactId {
    pub fn new(provider: &str, location: &Location) -> Self {
        // DefaultHasher with default keys is deterministic within a build, which
        // is all we need: ids are not persisted.
        let mut h = std::hash::DefaultHasher::new();
        provider.hash(&mut h);
        match location {
            Location::Paths { paths } => paths.hash(&mut h),
            Location::Managed { ids, .. } => ids.hash(&mut h),
        }
        Self(h.finish())
    }
}

impl std::fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// One cleanable thing.
#[derive(Clone, Debug)]
pub struct Artifact {
    pub id: ArtifactId,
    pub provider: &'static str,
    /// Which rule produced it, unique within the provider (`registry-src`).
    pub rule: &'static str,
    /// Human label (`registry/src`).
    pub label: String,
    pub origin: Origin,
    pub location: Location,
    pub safety: Safety,
    /// `None` until measured.
    pub size: Option<Size>,
    /// Newest mtime anywhere inside, `None` until measured.
    pub newest_mtime: Option<SystemTime>,
    /// The project a global artifact belongs to (a Bazel output base's workspace).
    pub owner: Option<PathBuf>,
    /// `owner` no longer exists.
    pub orphaned: bool,
    /// Shown instead of the path when the path means nothing to people.
    pub display: Option<String>,
}

impl Artifact {
    /// A new, unmeasured artifact.
    pub fn new(
        provider: &'static str,
        rule: &'static str,
        label: impl Into<String>,
        origin: Origin,
        location: Location,
        safety: Safety,
    ) -> Self {
        Self {
            id: ArtifactId::new(provider, &location),
            provider,
            rule,
            label: label.into(),
            origin,
            location,
            safety,
            size: None,
            newest_mtime: None,
            owner: None,
            orphaned: false,
            display: None,
        }
    }

    /// Paths on disk; empty for [`Location::Managed`].
    pub fn paths(&self) -> &[PathBuf] {
        match &self.location {
            Location::Paths { paths } => paths,
            Location::Managed { .. } => &[],
        }
    }

    /// The project this belongs to: where it was found, or its owner.
    pub fn project(&self) -> Option<&Path> {
        match &self.origin {
            Origin::Project { root, .. } => Some(root),
            Origin::Global => self.owner.as_deref(),
        }
    }

    /// On-disk bytes, 0 if not measured yet.
    pub fn on_disk(&self) -> u64 {
        self.size.map_or(0, |s| s.on_disk)
    }
}
