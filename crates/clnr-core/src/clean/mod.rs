//! Deleting artifacts: plan with [`Guard`] verdicts, then [`run`] the allowed part.

mod guard;
mod journal;
mod remove;

use rayon::prelude::*;

pub use guard::{Blocked, Guard};
pub use journal::Journal;
pub use remove::{Resumed, TOMBSTONE_PREFIX, resumable, resume};

use crate::model::{Artifact, ArtifactId, Location};

/// Selected artifacts with their guard verdicts.
pub struct Plan {
    pub items: Vec<(Artifact, Result<(), Blocked>)>,
}

impl Plan {
    pub fn new(guard: &Guard<'_>, artifacts: Vec<Artifact>) -> Self {
        let items = artifacts
            .into_iter()
            .map(|a| {
                let verdict = guard.check(&a);
                (a, verdict)
            })
            .collect();
        Self { items }
    }

    pub fn allowed(&self) -> impl Iterator<Item = &Artifact> {
        self.items.iter().filter(|(_, v)| v.is_ok()).map(|(a, _)| a)
    }

    pub fn blocked(&self) -> impl Iterator<Item = (&Artifact, &Blocked)> {
        self.items
            .iter()
            .filter_map(|(a, v)| v.as_ref().err().map(|b| (a, b)))
    }
}

#[derive(Debug)]
pub enum CleanEvent {
    Removed {
        id: ArtifactId,
        freed: u64,
    },
    Failed {
        id: ArtifactId,
        error: String,
    },
    /// The final guard check failed; nothing was touched.
    Blocked {
        id: ArtifactId,
        reason: Blocked,
    },
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Summary {
    pub removed: usize,
    pub freed: u64,
    pub failed: usize,
    pub blocked: usize,
}

impl From<&CleanEvent> for Summary {
    fn from(event: &CleanEvent) -> Self {
        match event {
            CleanEvent::Removed { freed, .. } => Self {
                removed: 1,
                freed: *freed,
                ..Self::default()
            },
            CleanEvent::Failed { .. } => Self {
                failed: 1,
                ..Self::default()
            },
            CleanEvent::Blocked { .. } => Self {
                blocked: 1,
                ..Self::default()
            },
        }
    }
}

impl std::ops::Add for Summary {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            removed: self.removed + other.removed,
            freed: self.freed + other.freed,
            failed: self.failed + other.failed,
            blocked: self.blocked + other.blocked,
        }
    }
}

/// Delete `artifacts` in parallel, re-checking each with `guard` immediately before. Blocks
/// until done; `on_event` is called from worker threads.
pub fn run(
    guard: &Guard<'_>,
    journal: &Journal,
    artifacts: &[&Artifact],
    on_event: &(dyn Fn(CleanEvent) + Sync),
) -> Summary {
    artifacts
        .par_iter()
        .map(|a| {
            let event = clean_one(guard, journal, a);
            let summary = Summary::from(&event);
            on_event(event);
            summary
        })
        .reduce(Summary::default, |a, b| a + b)
}

fn clean_one(guard: &Guard<'_>, journal: &Journal, artifact: &Artifact) -> CleanEvent {
    let id = artifact.id;
    if let Err(reason) = guard.check(artifact) {
        return CleanEvent::Blocked { id, reason };
    }
    let result = match &artifact.location {
        Location::Paths { paths } => paths.iter().try_for_each(|p| {
            remove::remove(p, id, journal).map_err(|e| format!("{}: {e}", p.display()))
        }),
        Location::Managed { name, .. } => match guard.provider(artifact) {
            Some(provider) => provider
                .delete(artifact, guard.env())
                .map_err(|e| format!("{} {name}: {e}", artifact.label)),
            None => Err(format!("unknown provider `{}`", artifact.provider)),
        },
    };
    if let Err(error) = result {
        return CleanEvent::Failed { id, error };
    }
    CleanEvent::Removed {
        id,
        freed: artifact.on_disk(),
    }
}
