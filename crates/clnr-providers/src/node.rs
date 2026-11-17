use clnr_core::{Env, GlobalSpec, Marker, Os, ProjectRule, Provider, Safety};

use crate::common::{default, global};

const PROJECT_RULES: &[ProjectRule] = &[
    ProjectRule {
        id: "node_modules",
        dir: "node_modules",
        marker: Marker::File("package.json"),
        verify: &[],
        safety: Safety::Redownload,
    },
    ProjectRule {
        id: "next",
        dir: ".next",
        marker: Marker::File("package.json"),
        verify: &["cache", "server", "BUILD_ID"],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "nuxt",
        dir: ".nuxt",
        marker: Marker::File("package.json"),
        verify: &[],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "turbo",
        dir: ".turbo",
        marker: Marker::File("package.json"),
        verify: &[],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "parcel",
        dir: ".parcel-cache",
        marker: Marker::File("package.json"),
        verify: &[],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "svelte-kit",
        dir: ".svelte-kit",
        marker: Marker::File("package.json"),
        verify: &[],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "angular",
        dir: ".angular",
        marker: Marker::File("package.json"),
        verify: &["cache"],
        safety: Safety::Regenerable,
    },
];

/// npm, yarn, pnpm, and bun.
pub struct Node;

impl Provider for Node {
    fn id(&self) -> &'static str {
        "node"
    }

    fn name(&self) -> &'static str {
        "Node.js"
    }

    fn tools(&self) -> &[&'static str] {
        &["node", "npm", "pnpm", "yarn", "bun"]
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        // npm's cache root may be configured anywhere, but only its distinctive `_cacache`,
        // `_npx`, and `_logs` subdirectories are offered.
        let npm = env
            .path_var("npm_config_cache")
            .or_else(|| env.path_var("NPM_CONFIG_CACHE"))
            .unwrap_or_else(|| env.home().join(".npm"));
        let yarn = env.dir_or("YARN_CACHE_FOLDER", || match env.os() {
            Os::MacOs => env.cache_dir().join("Yarn"),
            _ => env.cache_dir().join("yarn"),
        });
        let pnpm = match env.os() {
            Os::MacOs => env.home().join("Library/pnpm/store"),
            _ => env.data_dir().join("pnpm/store"),
        };

        vec![
            global(
                "npm-cacache",
                "npm cache",
                default(npm.join("_cacache")),
                Safety::Redownload,
            ),
            global(
                "npm-npx",
                "npx cache",
                default(npm.join("_npx")),
                Safety::Redownload,
            ),
            global(
                "npm-logs",
                "npm logs",
                default(npm.join("_logs")),
                Safety::Regenerable,
            ),
            GlobalSpec {
                // Yarn classic keeps one directory per cache format version.
                verify: &["v6", "v5", "v4"],
                ..global("yarn", "yarn cache", yarn, Safety::Redownload)
            },
            global(
                "yarn-berry",
                "yarn berry cache",
                default(env.home().join(".yarn/berry/cache")),
                Safety::Redownload,
            ),
            GlobalSpec {
                verify: &["v10", "v3"],
                ..global(
                    "pnpm-store",
                    "pnpm store",
                    default(pnpm),
                    Safety::Redownload,
                )
            },
            // Default location only: `BUN_INSTALL_CACHE_DIR` can point anywhere and the cache
            // has no marker to verify it by.
            global(
                "bun",
                "bun cache",
                default(env.home().join(".bun/install/cache")),
                Safety::Redownload,
            ),
        ]
    }

    fn project_rules(&self) -> &[ProjectRule] {
        PROJECT_RULES
    }
}
