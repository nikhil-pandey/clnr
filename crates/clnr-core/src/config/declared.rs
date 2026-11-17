//! Providers defined in the config file.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{CustomProvider, CustomRule, MarkerConfig};
use crate::env::{Env, Source};
use crate::model::Safety;
use crate::provider::{GlobalSpec, Marker, ProjectRule, Provider};

struct Declared {
    id: &'static str,
    name: &'static str,
    tools: &'static [&'static str],
    /// The config file, shown as the source of every location.
    source: PathBuf,
    globals: Vec<DeclaredGlobal>,
    rules: Vec<ProjectRule>,
}

struct DeclaredGlobal {
    rule: &'static str,
    label: String,
    path: PathBuf,
    safety: Safety,
    verify: &'static [&'static str],
}

impl Provider for Declared {
    fn id(&self) -> &'static str {
        self.id
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn tools(&self) -> &[&'static str] {
        self.tools
    }

    fn globals(&self, _env: &Env) -> Vec<GlobalSpec> {
        self.globals
            .iter()
            .map(|g| GlobalSpec {
                rule: g.rule,
                label: g.label.clone(),
                path: g.path.clone(),
                safety: g.safety,
                source: Source::Config(self.source.clone()),
                verify: g.verify,
                owner: None,
                display: None,
            })
            .collect()
    }

    fn project_rules(&self) -> &[ProjectRule] {
        &self.rules
    }
}

/// Check one `[[provider]]` table and build it. Problems are appended; `None` if any.
pub(super) fn build(
    custom: CustomProvider,
    index: usize,
    source: &Path,
    env: &Env,
    builtin: &HashSet<&str>,
    problems: &mut Vec<String>,
) -> Option<Box<dyn Provider>> {
    let at = format!("provider[{index}] `{}`", custom.id);
    let before = problems.len();

    if !is_valid_id(&custom.id) {
        problems.push(format!(
            "{at}: id must be lowercase letters, digits, and dashes"
        ));
    }
    if builtin.contains(custom.id.as_str()) {
        problems.push(format!("{at}: clashes with a built-in provider"));
    }

    let mut globals = Vec::new();
    for (i, g) in custom.global.into_iter().enumerate() {
        let at = format!("{at} global[{i}]");
        let path = env.expand(&g.path);
        if !path.is_absolute() {
            problems.push(format!(
                "{at}: `{}` must be absolute or start with ~",
                g.path
            ));
            continue;
        }
        // Covers `/` and `~` too.
        if env.home().starts_with(&path) {
            problems.push(format!(
                "{at}: `{}` is or contains the home directory",
                g.path
            ));
            continue;
        }
        if g.verify.is_empty() || g.verify.iter().any(String::is_empty) {
            problems.push(format!(
                "{at}: `verify` must list at least one entry only this cache contains"
            ));
        }
        let label = g.label.unwrap_or_else(|| {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        globals.push(DeclaredGlobal {
            rule: leak(g.id.unwrap_or_else(|| format!("global-{i}"))),
            label,
            path,
            safety: g.safety,
            verify: leak_list(g.verify),
        });
    }

    let mut rule_ids = HashSet::new();
    let mut rules = Vec::new();
    for (i, r) in custom.project.into_iter().enumerate() {
        let at = format!("{at} project[{i}]");
        if let Some(problem) = check_rule(&r) {
            problems.push(format!("{at}: {problem}"));
            continue;
        }
        let id = r.id.clone().unwrap_or_else(|| r.dir.clone());
        if !rule_ids.insert(id.clone()) {
            problems.push(format!("{at}: rule `{id}` is defined twice"));
            continue;
        }
        rules.push(ProjectRule {
            id: leak(id),
            dir: leak(r.dir),
            marker: match r.marker {
                MarkerConfig::File(name) => Marker::File(leak(name)),
                MarkerConfig::AnyFile(names) => Marker::AnyFile(leak_list(names)),
                MarkerConfig::Suffix(suffixes) => Marker::AnySuffix(leak_list(suffixes)),
                MarkerConfig::Always => Marker::Always,
            },
            verify: leak_list(r.verify),
            safety: r.safety,
        });
    }

    if problems.len() > before {
        return None;
    }
    let id = leak(custom.id);
    Some(Box::new(Declared {
        id,
        name: custom.name.map_or(id, leak),
        tools: leak_list(custom.tools),
        source: source.to_owned(),
        globals,
        rules,
    }))
}

fn check_rule(r: &CustomRule) -> Option<&'static str> {
    if r.dir.is_empty() || r.dir == "." || r.dir == ".." || r.dir.contains('/') {
        return Some("`dir` must be a single directory name");
    }
    if matches!(r.marker, MarkerConfig::Always) && r.verify.is_empty() {
        return Some("`marker = \"always\"` needs a `verify` list to identify the directory");
    }
    let empty_marker = match &r.marker {
        MarkerConfig::File(name) => name.is_empty(),
        MarkerConfig::AnyFile(list) | MarkerConfig::Suffix(list) => {
            list.is_empty() || list.iter().any(String::is_empty)
        }
        MarkerConfig::Always => false,
    };
    empty_marker.then_some("`marker` must not be empty")
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

// Provider data is `'static` (built-ins are constants). Config is read once per process, so
// its strings are leaked to match rather than threading lifetimes through every type.

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn leak_list(list: Vec<String>) -> &'static [&'static str] {
    Box::leak(
        list.into_iter()
            .map(leak)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}
