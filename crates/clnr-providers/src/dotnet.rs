use clnr_core::{Env, GlobalSpec, Marker, ProjectRule, Provider, Safety};

use crate::common::{default, global};

const PROJECT_FILES: &[&str] = &[".csproj", ".fsproj", ".vbproj"];

const PROJECT_RULES: &[ProjectRule] = &[
    ProjectRule {
        id: "obj",
        dir: "obj",
        marker: Marker::AnySuffix(PROJECT_FILES),
        verify: &["project.assets.json"],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        // `bin` is a common name, so it needs both a project file next to it and MSBuild's
        // configuration directories inside.
        id: "bin",
        dir: "bin",
        marker: Marker::AnySuffix(PROJECT_FILES),
        verify: &["Debug", "Release"],
        safety: Safety::Regenerable,
    },
];

/// .NET build output and NuGet caches.
pub struct Dotnet;

impl Provider for Dotnet {
    fn id(&self) -> &'static str {
        "dotnet"
    }

    fn name(&self) -> &'static str {
        ".NET"
    }

    fn tools(&self) -> &[&'static str] {
        &["dotnet"]
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        // Default locations only: NUGET_PACKAGES and friends can point anywhere and these
        // caches have no marker to verify them by. NuGet uses ~/.local/share on macOS too.
        let nuget = env.home().join(".local/share/NuGet");
        vec![
            global(
                "nuget-packages",
                "nuget packages",
                default(env.home().join(".nuget/packages")),
                Safety::Redownload,
            ),
            global(
                "nuget-http-cache",
                "nuget http cache",
                default(nuget.join("http-cache")),
                Safety::Redownload,
            ),
            global(
                "nuget-v3-cache",
                "nuget v3 cache",
                default(nuget.join("v3-cache")),
                Safety::Redownload,
            ),
            global(
                "nuget-plugins-cache",
                "nuget plugins cache",
                default(nuget.join("plugins-cache")),
                Safety::Regenerable,
            ),
        ]
    }

    fn project_rules(&self) -> &[ProjectRule] {
        PROJECT_RULES
    }
}
