use std::time::{Duration, SystemTime};

use crate::model::{Artifact, Location, Safety};

/// Narrows a set of artifacts. Empty fields match everything.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// At least this many bytes on disk.
    pub min_size: Option<u64>,
    /// Nothing inside modified more recently than this.
    pub older_than: Option<Duration>,
    /// Only these safety classes.
    pub safety: Vec<Safety>,
    /// Only artifacts whose owning project is gone.
    pub orphans: bool,
    /// Case-insensitive substring of provider, label, location, or owning project.
    pub text: Option<String>,
}

impl Filter {
    /// Does `artifact` pass? Unmeasured artifacts fail size and age conditions.
    pub fn matches(&self, artifact: &Artifact, now: SystemTime) -> bool {
        let size_ok = self
            .min_size
            .is_none_or(|min| artifact.size.is_some_and(|s| s.on_disk >= min));
        let age_ok = self.older_than.is_none_or(|age| {
            artifact
                .newest_mtime
                .is_some_and(|m| now.duration_since(m).is_ok_and(|d| d >= age))
        });
        let safety_ok = self.safety.is_empty() || self.safety.contains(&artifact.safety);
        let orphan_ok = !self.orphans || artifact.orphaned;
        let text_ok = self
            .text
            .as_deref()
            .is_none_or(|t| haystack(artifact).contains(&t.to_lowercase()));
        size_ok && age_ok && safety_ok && orphan_ok && text_ok
    }
}

fn haystack(a: &Artifact) -> String {
    let location = match &a.location {
        Location::Paths { paths } => paths.iter().map(|p| p.display().to_string()).collect(),
        Location::Managed { name, .. } => vec![name.clone()],
    };
    let owner = a
        .owner
        .as_ref()
        .map(|o| o.display().to_string())
        .unwrap_or_default();
    format!("{} {} {} {owner}", a.provider, a.label, location.join(" ")).to_lowercase()
}
