use std::collections::HashSet;

use super::GlobalScope;
use crate::env::Env;
use crate::model::{Artifact, Location, Origin};
use crate::provider::verified;
use crate::registry::Registry;

/// Every provider's global locations that exist on disk, unmeasured, with canonical paths.
///
/// Symlinked locations and ones failing their `verify` check are skipped, and specs resolving
/// to the same directory produce one artifact.
pub(super) fn resolve(registry: &Registry, env: &Env) -> (Vec<Artifact>, Vec<GlobalScope>) {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut scope = Vec::new();
    for provider in registry.providers() {
        for spec in provider.globals(env) {
            let is_real =
                std::fs::symlink_metadata(&spec.path).is_ok_and(|m| !m.file_type().is_symlink());
            if !is_real {
                continue;
            }
            let Ok(path) = spec.path.canonicalize() else {
                continue;
            };
            if !verified(&path, spec.verify) || !seen.insert(path.clone()) {
                continue;
            }
            scope.push(GlobalScope {
                path: path.clone(),
                verify: spec.verify,
            });
            let mut artifact = Artifact::new(
                provider.id(),
                spec.rule,
                spec.label,
                Origin::Global,
                Location::path(path),
                spec.safety,
            );
            artifact.orphaned = spec.owner.as_ref().is_some_and(|o| !o.exists());
            artifact.owner = spec.owner;
            artifact.display = spec.display;
            out.push(artifact);
        }
    }
    (out, scope)
}
