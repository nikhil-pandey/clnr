use clnr_core::{Env, GlobalSpec, Provider, Safety, Source};

use crate::common::global;

/// Go module and build caches.
pub struct Go;

impl Provider for Go {
    fn id(&self) -> &'static str {
        "go"
    }

    fn name(&self) -> &'static str {
        "Go"
    }

    fn tools(&self) -> &[&'static str] {
        &["go"]
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        // GOPATH may list several directories; Go uses the first.
        let gopath = env
            .var("GOPATH")
            .and_then(|v| std::env::split_paths(v).next())
            .filter(|p| p.is_absolute());
        let modcache = match (env.path_var("GOMODCACHE"), gopath) {
            (Some(p), _) => (p, Source::Env("GOMODCACHE")),
            (None, Some(p)) => (p.join("pkg/mod"), Source::Env("GOPATH")),
            (None, None) => (env.home().join("go/pkg/mod"), Source::Default),
        };
        let build = env.dir_or("GOCACHE", || env.cache_dir().join("go-build"));

        vec![
            GlobalSpec {
                // Read-only files by design; removal makes directories writable first.
                verify: &["cache/download"],
                ..global("modcache", "module cache", modcache, Safety::Redownload)
            },
            GlobalSpec {
                verify: &["trim.txt"],
                ..global("build-cache", "build cache", build, Safety::Regenerable)
            },
        ]
    }
}
