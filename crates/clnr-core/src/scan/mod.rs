//! Discovering and measuring artifacts.
//!
//! A [`Scan`] resolves provider globals synchronously (a few `stat`s), then walks the roots and
//! measures everything on background threads, streaming [`ScanEvent`]s. Callers either consume
//! the events live (TUI) or [`Scan::wait`] for a [`ScanReport`] (headless).

mod globals;
mod measure;
mod walk;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crossbeam_channel::{Receiver, Sender, unbounded};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::iter::{ParallelBridge, ParallelIterator};

pub use measure::{Measurement, measure, measure_paths};

use crate::env::Env;
use crate::model::{Artifact, ArtifactId};
use crate::registry::Registry;

#[derive(Debug)]
pub enum ScanEvent {
    Found(Box<Artifact>),
    Measured {
        id: ArtifactId,
        measurement: Measurement,
    },
    /// A path could not be read. The scan carries on.
    Error(String),
    /// Walking and measuring are finished. Always the last event.
    Done,
}

/// What to scan.
#[derive(Clone, Debug, Default)]
pub struct ScanOptions {
    /// Searched for project artifacts. Missing ones are skipped.
    pub roots: Vec<PathBuf>,
    /// Never walked.
    pub exclude: Excludes,
    /// Never deleted, nor anything inside or containing them.
    pub protect: Vec<PathBuf>,
}

/// Glob patterns for paths the walk skips. A plain path also excludes everything below it.
#[derive(Clone, Debug)]
pub struct Excludes {
    set: GlobSet,
    patterns: Vec<String>,
}

impl Excludes {
    /// Compile absolute-path patterns (`/home/me/code/archive`, `**/vendor`).
    pub fn new(patterns: &[String]) -> Result<Self, globset::Error> {
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let pattern = pattern.trim_end_matches('/');
            builder.add(Glob::new(pattern)?);
            builder.add(Glob::new(&format!("{pattern}/**"))?);
        }
        Ok(Self {
            set: builder.build()?,
            patterns: patterns.to_vec(),
        })
    }

    pub fn is_match(&self, path: &Path) -> bool {
        self.set.is_match(path)
    }

    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }
}

impl Default for Excludes {
    fn default() -> Self {
        Self {
            set: GlobSet::empty(),
            patterns: Vec::new(),
        }
    }
}

/// What a scan covered. Deletes are only allowed inside it.
///
/// All paths are canonical.
#[derive(Clone, Debug, Default)]
pub struct ScanScope {
    /// Roots that were walked for project artifacts.
    pub roots: Vec<PathBuf>,
    /// Global locations resolved by providers for this run.
    pub globals: Vec<GlobalScope>,
    /// Never deleted, nor anything inside or containing them.
    pub protect: Vec<PathBuf>,
}

/// A global location a scan resolved, with the identity check it must keep passing.
#[derive(Clone, Debug)]
pub struct GlobalScope {
    pub path: PathBuf,
    pub verify: &'static [&'static str],
}

/// A running scan.
pub struct Scan {
    events: Receiver<ScanEvent>,
    scope: ScanScope,
    cancel: Arc<AtomicBool>,
}

/// The outcome of a finished scan.
pub struct ScanReport {
    pub artifacts: Vec<Artifact>,
    pub errors: Vec<String>,
    pub scope: ScanScope,
}

impl Scan {
    /// Start scanning the roots plus every provider's globals.
    pub fn start(registry: Arc<Registry>, env: &Env, options: &ScanOptions) -> Self {
        let roots = canonical_roots(&options.roots);
        let (globals, global_scope) = globals::resolve(&registry, env);
        let scope = ScanScope {
            roots: roots.clone(),
            globals: global_scope,
            // Canonical where possible, so a symlinked spelling still protects.
            protect: options
                .protect
                .iter()
                .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
                .collect(),
        };
        let (tx, events) = unbounded();
        let cancel = Arc::new(AtomicBool::new(false));

        let worker = Worker {
            registry,
            env: env.clone(),
            roots,
            skip: scope.globals.iter().map(|g| g.path.clone()).collect(),
            exclude: options.exclude.clone(),
            tx,
            cancel: Arc::clone(&cancel),
        };
        thread::spawn(move || worker.run(globals));

        Self {
            events,
            scope,
            cancel,
        }
    }

    pub fn events(&self) -> &Receiver<ScanEvent> {
        &self.events
    }

    pub fn scope(&self) -> &ScanScope {
        &self.scope
    }

    /// Ask the scan to stop early. A [`ScanEvent::Done`] still follows.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Block until the scan is done and return everything it found, measured.
    pub fn wait(self) -> ScanReport {
        let mut artifacts = Vec::new();
        let mut index = HashMap::new();
        let mut errors = Vec::new();
        for event in self.events.iter() {
            match event {
                ScanEvent::Found(a) => {
                    index.insert(a.id, artifacts.len());
                    artifacts.push(*a);
                }
                ScanEvent::Measured { id, measurement } => {
                    if let Some(&i) = index.get(&id) {
                        let a: &mut Artifact = &mut artifacts[i];
                        a.size = Some(measurement.size);
                        a.newest_mtime = measurement.newest_mtime;
                    }
                }
                ScanEvent::Error(e) => errors.push(e),
                ScanEvent::Done => break,
            }
        }
        ScanReport {
            artifacts,
            errors,
            scope: self.scope,
        }
    }
}

/// Background half of a [`Scan`].
struct Worker {
    registry: Arc<Registry>,
    env: Env,
    roots: Vec<PathBuf>,
    /// Global locations; the walk does not descend into them.
    skip: Vec<PathBuf>,
    exclude: Excludes,
    tx: Sender<ScanEvent>,
    cancel: Arc<AtomicBool>,
}

impl Worker {
    fn run(self, globals: Vec<Artifact>) {
        let Self {
            registry,
            env,
            roots,
            skip,
            exclude,
            tx,
            cancel,
        } = self;
        let (tx, cancel, env, registry) = (&tx, &*cancel, &env, &*registry);
        let (measure_tx, measure_rx) = unbounded::<(ArtifactId, Vec<PathBuf>)>();

        thread::scope(|s| {
            // Measure artifacts as they are found, in parallel, while the walk continues.
            s.spawn(move || {
                measure_rx.into_iter().par_bridge().for_each(|(id, paths)| {
                    if !cancel.load(Ordering::Relaxed) {
                        let measurement = measure_paths(&paths);
                        let _ = tx.send(ScanEvent::Measured { id, measurement });
                    }
                });
            });

            // Owns `measure_tx`; dropping it when this closure returns lets the measurer finish.
            // `Found` goes out before the artifact is queued for measuring, so consumers
            // always see an artifact before its measurement.
            let found = move |a: Artifact| {
                let job = (a.id, a.paths().to_vec());
                let _ = tx.send(ScanEvent::Found(Box::new(a)));
                // Managed artifacts come sized by their tool.
                if !job.1.is_empty() {
                    let _ = measure_tx.send(job);
                }
            };
            globals.into_iter().for_each(&found);

            // Tool-backed discovery (Docker, ...) runs alongside the walk.
            // These are managed artifacts, sized by their tool, so they skip the measurer.
            for provider in registry.providers() {
                s.spawn(move || match provider.discover(env) {
                    Ok(artifacts) => {
                        for a in artifacts {
                            let _ = tx.send(ScanEvent::Found(Box::new(a)));
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(ScanEvent::Error(format!("{}: {e}", provider.id())));
                    }
                });
            }
            walk::walk(&walk::Walk {
                registry,
                roots: &roots,
                skip: &skip,
                exclude: &exclude,
                cancel,
                found: &found,
                error: &|e| {
                    let _ = tx.send(ScanEvent::Error(e));
                },
            });
        });

        let _ = tx.send(ScanEvent::Done);
    }
}

/// Canonicalize roots, dropping missing ones, duplicates, and roots inside other roots.
fn canonical_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut canonical: Vec<PathBuf> = roots.iter().filter_map(|r| r.canonicalize().ok()).collect();
    canonical.sort();
    canonical.dedup();
    let mut kept: Vec<PathBuf> = Vec::new();
    for root in canonical {
        if !kept.iter().any(|k| root.starts_with(k)) {
            kept.push(root);
        }
    }
    kept
}
