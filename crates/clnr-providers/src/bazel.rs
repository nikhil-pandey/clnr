use std::fs;
use std::path::{Path, PathBuf};

use clnr_core::env::process_alive;
use clnr_core::{Artifact, Env, GlobalSpec, Os, Provider, Safety};

use crate::common::{default, global, subdirs};

const OUTPUT_BASE: &str = "output-base";
const INSTALL_BASE: &str = "install-base";

/// Bazel output bases, install bases, the repository cache, and bazelisk downloads.
pub struct Bazel;

impl Bazel {
    /// Bazel's default `--output_user_root`. A root set in a bazelrc is not read.
    fn output_user_root(env: &Env) -> Option<PathBuf> {
        let user = match env.var("USER") {
            Some(user) => user.to_owned(),
            None => env.home().file_name()?.to_string_lossy().into_owned(),
        };
        Some(match env.os() {
            Os::MacOs => PathBuf::from(format!("/private/var/tmp/_bazel_{user}")),
            _ => env.home().join(format!(".cache/bazel/_bazel_{user}")),
        })
    }
}

impl Provider for Bazel {
    fn id(&self) -> &'static str {
        "bazel"
    }

    fn name(&self) -> &'static str {
        "Bazel"
    }

    fn tools(&self) -> &[&'static str] {
        &["bazel", "bazelisk"]
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        let mut out = Vec::new();
        if let Some(root) = Self::output_user_root(env) {
            // One output base per workspace, named by the MD5 of its path.
            for base in subdirs(&root).filter(|p| is_md5_name(p)) {
                // Shown by its workspace, which is the owner.
                out.push(GlobalSpec {
                    verify: &["DO_NOT_BUILD_HERE"],
                    owner: workspace_of(&base),
                    ..global(
                        OUTPUT_BASE,
                        "output base",
                        default(base),
                        Safety::Regenerable,
                    )
                });
            }
            // One install base per Bazel version.
            for base in subdirs(&root.join("install")).filter(|p| is_md5_name(p)) {
                // Shown by its Bazel version; the directory is a hash.
                let version = fs::read_to_string(base.join("build-label.txt")).ok();
                out.push(GlobalSpec {
                    verify: &["A-server.jar", "build-label.txt"],
                    display: version.map(|v| format!("bazel {}", v.trim())),
                    ..global(
                        INSTALL_BASE,
                        "install base",
                        default(base),
                        Safety::Regenerable,
                    )
                });
            }
            out.push(global(
                "repo-cache",
                "repository cache",
                default(root.join("cache/repos")),
                Safety::Redownload,
            ));
        }

        let bazelisk = env.dir_or("BAZELISK_HOME", || env.cache_dir().join("bazelisk"));
        out.push(GlobalSpec {
            verify: &["downloads"],
            ..global(
                "bazelisk",
                "bazelisk downloads",
                bazelisk,
                Safety::Redownload,
            )
        });
        out
    }

    fn in_use(&self, artifact: &Artifact) -> Option<String> {
        let path = artifact.paths().first()?;
        match artifact.rule {
            OUTPUT_BASE => live_server(path).map(|pid| {
                let workspace = artifact
                    .owner
                    .as_ref()
                    .map_or_else(String::new, |w| format!(" in {}", w.display()));
                format!("bazel server running (pid {pid}); run `bazel shutdown`{workspace}")
            }),
            // Servers run from an install base; don't pull one out from under them.
            INSTALL_BASE => {
                let root = path.parent()?.parent()?;
                subdirs(root)
                    .filter(|p| is_md5_name(p))
                    .find_map(|base| live_server(&base))
                    .map(|pid| format!("a bazel server is running (pid {pid})"))
            }
            _ => None,
        }
    }
}

fn is_md5_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.len() == 32 && n.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The workspace an output base belongs to, as recorded by Bazel.
fn workspace_of(output_base: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(output_base.join("DO_NOT_BUILD_HERE")).ok()?;
    let path = PathBuf::from(text.trim());
    path.is_absolute().then_some(path)
}

/// The pid of a running server for this output base.
fn live_server(output_base: &Path) -> Option<u32> {
    let pid = fs::read_to_string(output_base.join("server/server.pid.txt")).ok()?;
    let pid: u32 = pid.trim().parse().ok()?;
    process_alive(pid).then_some(pid)
}
