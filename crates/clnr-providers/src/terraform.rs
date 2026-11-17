use clnr_core::{Env, GlobalSpec, Marker, ProjectRule, Provider, Safety, Source};

use crate::common::global;

const CONFIG_SUFFIXES: &[&str] = &[".tf", ".tf.json", ".tofu"];

const PROJECT_RULES: &[ProjectRule] = &[
    ProjectRule {
        // Providers, modules, and backend settings. Real state (`terraform.tfstate`,
        // `terraform.tfstate.d/`) lives next to it, outside the artifact.
        id: "dot-terraform",
        dir: ".terraform",
        marker: Marker::AnySuffix(CONFIG_SUFFIXES),
        verify: &["providers", "modules", "terraform.tfstate", "environment"],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "terragrunt-cache",
        dir: ".terragrunt-cache",
        marker: Marker::File("terragrunt.hcl"),
        verify: &[],
        safety: Safety::Regenerable,
    },
];

/// Terraform, OpenTofu, and Terragrunt.
pub struct Terraform;

impl Provider for Terraform {
    fn id(&self) -> &'static str {
        "terraform"
    }

    fn name(&self) -> &'static str {
        "Terraform"
    }

    fn tools(&self) -> &[&'static str] {
        &["terraform", "tofu", "terragrunt"]
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        // There is no default plugin cache; it exists only when configured.
        let Some(path) = env.path_var("TF_PLUGIN_CACHE_DIR") else {
            return vec![];
        };
        vec![GlobalSpec {
            verify: &["registry.terraform.io", "registry.opentofu.org"],
            ..global(
                "plugin-cache",
                "plugin cache",
                (path, Source::Env("TF_PLUGIN_CACHE_DIR")),
                Safety::Redownload,
            )
        }]
    }

    fn project_rules(&self) -> &[ProjectRule] {
        PROJECT_RULES
    }
}
