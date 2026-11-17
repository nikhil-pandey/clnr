//! Overrides on existing providers: disable one, disable rules, or make rules stricter.

use std::collections::{HashMap, HashSet};

use super::ProviderOverride;
use crate::env::Env;
use crate::model::{Artifact, Safety};
use crate::provider::{Detection, GlobalSpec, ProjectRule, Provider};

/// Apply `ov` to `provider`. `None` if the provider is disabled. Problems are appended.
pub(super) fn apply(
    provider: Box<dyn Provider>,
    ov: &ProviderOverride,
    env: &Env,
    problems: &mut Vec<String>,
) -> Option<Box<dyn Provider>> {
    let at = format!("providers.{}", provider.id());

    // Rule ids and their current safety, from both kinds of rules.
    let mut known: HashMap<&str, Safety> = provider
        .project_rules()
        .iter()
        .map(|r| (r.id, r.safety))
        .collect();
    let globals = provider.globals(env);
    for g in &globals {
        known.entry(g.rule).or_insert(g.safety);
    }

    let mut disabled = HashSet::new();
    let mut safety = HashMap::new();
    for (rule, r) in &ov.rules {
        let Some(&current) = known.get(rule.as_str()) else {
            problems.push(format!("{at}.rules.{rule}: no such rule"));
            continue;
        };
        if r.enabled == Some(false) {
            disabled.insert(rule.clone());
        }
        if let Some(s) = r.safety {
            if s < current {
                problems.push(format!(
                    "{at}.rules.{rule}: safety can only be made stricter than `{}`",
                    current.as_str()
                ));
            } else {
                safety.insert(rule.clone(), s);
            }
        }
    }

    if ov.enabled == Some(false) {
        return None;
    }
    if disabled.is_empty() && safety.is_empty() {
        return Some(provider);
    }
    Some(Box::new(Overridden::new(provider, disabled, safety)))
}

struct Overridden {
    inner: Box<dyn Provider>,
    /// The inner provider's project rules, filtered and adjusted.
    rules: Vec<ProjectRule>,
    disabled: HashSet<String>,
    safety: HashMap<String, Safety>,
}

impl Overridden {
    fn new(
        inner: Box<dyn Provider>,
        disabled: HashSet<String>,
        safety: HashMap<String, Safety>,
    ) -> Self {
        let mut this = Self {
            rules: Vec::new(),
            inner,
            disabled,
            safety,
        };
        this.rules = this
            .inner
            .project_rules()
            .iter()
            .filter(|r| this.enabled(r.id))
            .map(|r| ProjectRule {
                safety: this.adjust(r.id, r.safety),
                ..r.clone()
            })
            .collect();
        this
    }

    fn enabled(&self, rule: &str) -> bool {
        !self.disabled.contains(rule)
    }

    fn adjust(&self, rule: &str, safety: Safety) -> Safety {
        self.safety.get(rule).map_or(safety, |&s| safety.max(s))
    }
}

impl Provider for Overridden {
    fn id(&self) -> &'static str {
        self.inner.id()
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn tools(&self) -> &[&'static str] {
        self.inner.tools()
    }

    fn detect(&self, env: &Env) -> Detection {
        self.inner.detect(env)
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        self.inner
            .globals(env)
            .into_iter()
            .filter(|g| self.enabled(g.rule))
            .map(|g| GlobalSpec {
                safety: self.adjust(g.rule, g.safety),
                ..g
            })
            .collect()
    }

    fn project_rules(&self) -> &[ProjectRule] {
        &self.rules
    }

    fn discover(&self, env: &Env) -> Result<Vec<Artifact>, String> {
        Ok(self
            .inner
            .discover(env)?
            .into_iter()
            .filter(|a| self.enabled(a.rule))
            .map(|mut a| {
                a.safety = self.adjust(a.rule, a.safety);
                a
            })
            .collect())
    }

    fn delete(&self, artifact: &Artifact, env: &Env) -> Result<(), String> {
        self.inner.delete(artifact, env)
    }

    fn in_use(&self, artifact: &Artifact) -> Option<String> {
        self.inner.in_use(artifact)
    }
}
