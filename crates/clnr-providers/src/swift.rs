use clnr_core::{Env, GlobalSpec, Marker, Os, ProjectRule, Provider, Safety};

use crate::common::{default, global};

const PROJECT_RULES: &[ProjectRule] = &[
    ProjectRule {
        id: "swiftpm-build",
        dir: ".build",
        marker: Marker::File("Package.swift"),
        verify: &[
            "debug",
            "release",
            "checkouts",
            "repositories",
            "workspace-state.json",
        ],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "pods",
        dir: "Pods",
        marker: Marker::File("Podfile"),
        verify: &["Manifest.lock"],
        safety: Safety::Redownload,
    },
];

/// Swift Package Manager and CocoaPods.
pub struct Swift;

impl Provider for Swift {
    fn id(&self) -> &'static str {
        "swift"
    }

    fn name(&self) -> &'static str {
        "Swift"
    }

    fn tools(&self) -> &[&'static str] {
        &["swift", "pod"]
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        let mut out = vec![global(
            "swiftpm-cache",
            "swiftpm cache",
            default(env.cache_dir().join("org.swift.swiftpm")),
            Safety::Redownload,
        )];
        if env.os() == Os::MacOs {
            out.push(global(
                "cocoapods-cache",
                "cocoapods cache",
                default(env.cache_dir().join("CocoaPods")),
                Safety::Redownload,
            ));
        }
        out
    }

    fn project_rules(&self) -> &[ProjectRule] {
        PROJECT_RULES
    }
}
