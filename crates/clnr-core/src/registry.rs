use std::collections::HashMap;
use std::fmt;

use crate::provider::{ProjectRule, Provider};

/// The set of providers a run works with, plus an index from directory name to the project
/// rules that look for it. The walker does one lookup per directory regardless of how many
/// providers are registered.
pub struct Registry {
    providers: Vec<Box<dyn Provider>>,
    /// Directory name → (provider index, rule index).
    by_dir: HashMap<&'static str, Vec<(usize, usize)>>,
}

impl Registry {
    pub fn new(providers: Vec<Box<dyn Provider>>) -> Self {
        let mut registry = Self {
            providers,
            by_dir: HashMap::new(),
        };
        registry.reindex();
        registry
    }

    fn reindex(&mut self) {
        self.by_dir.clear();
        for (p, provider) in self.providers.iter().enumerate() {
            for (r, rule) in provider.project_rules().iter().enumerate() {
                self.by_dir.entry(rule.dir).or_default().push((p, r));
            }
        }
    }

    pub fn providers(&self) -> &[Box<dyn Provider>] {
        &self.providers
    }

    pub fn provider(&self, id: &str) -> Option<&dyn Provider> {
        self.providers
            .iter()
            .find(|p| p.id() == id)
            .map(AsRef::as_ref)
    }

    /// Project rules that look for a directory with this name.
    pub fn rules_for_dir(&self, name: &str) -> impl Iterator<Item = (&dyn Provider, &ProjectRule)> {
        self.by_dir.get(name).into_iter().flatten().map(|&(p, r)| {
            let provider = self.providers[p].as_ref();
            (provider, &provider.project_rules()[r])
        })
    }

    /// Look up a project rule by provider and rule id.
    pub fn project_rule(&self, provider: &str, rule: &str) -> Option<&ProjectRule> {
        self.provider(provider)?
            .project_rules()
            .iter()
            .find(|r| r.id == rule)
    }

    /// Keep only the providers with these ids. Errors on an unknown id.
    pub fn retain_ids(&mut self, ids: &[String]) -> Result<(), UnknownProvider> {
        if let Some(bad) = ids.iter().find(|id| self.provider(id).is_none()) {
            return Err(UnknownProvider {
                id: bad.clone(),
                known: self.providers.iter().map(|p| p.id()).collect(),
            });
        }
        self.providers.retain(|p| ids.iter().any(|id| id == p.id()));
        self.reindex();
        Ok(())
    }
}

#[derive(Debug)]
pub struct UnknownProvider {
    pub id: String,
    pub known: Vec<&'static str>,
}

impl fmt::Display for UnknownProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown provider `{}` (known: {})",
            self.id,
            self.known.join(", ")
        )
    }
}

impl std::error::Error for UnknownProvider {}
