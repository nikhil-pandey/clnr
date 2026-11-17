use clnr_core::{Env, GlobalSpec, Marker, ProjectRule, Provider, Safety};

use crate::common::{default, global};

const PROJECT_FILES: &[&str] = &[
    "pyproject.toml",
    "requirements.txt",
    "setup.py",
    "setup.cfg",
    "Pipfile",
    "uv.lock",
];

/// Tools that tag their cache directories with `CACHEDIR.TAG`.
const TAGGED: &[&str] = &["CACHEDIR.TAG"];

const PROJECT_RULES: &[ProjectRule] = &[
    ProjectRule {
        id: "venv",
        dir: ".venv",
        marker: Marker::AnyFile(PROJECT_FILES),
        verify: &["pyvenv.cfg"],
        safety: Safety::Caution,
    },
    ProjectRule {
        id: "venv-plain",
        dir: "venv",
        marker: Marker::AnyFile(PROJECT_FILES),
        verify: &["pyvenv.cfg"],
        safety: Safety::Caution,
    },
    ProjectRule {
        id: "tox",
        dir: ".tox",
        marker: Marker::AnyFile(&["tox.ini", "pyproject.toml", "setup.cfg"]),
        verify: &[],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "nox",
        dir: ".nox",
        marker: Marker::File("noxfile.py"),
        verify: &[],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "pytest-cache",
        dir: ".pytest_cache",
        marker: Marker::Always,
        verify: TAGGED,
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "mypy-cache",
        dir: ".mypy_cache",
        marker: Marker::Always,
        verify: TAGGED,
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "ruff-cache",
        dir: ".ruff_cache",
        marker: Marker::Always,
        verify: TAGGED,
        safety: Safety::Regenerable,
    },
];

/// pip, uv, Poetry, pre-commit, Hugging Face, and project virtualenvs and tool caches.
pub struct Python;

impl Provider for Python {
    fn id(&self) -> &'static str {
        "python"
    }

    fn name(&self) -> &'static str {
        "Python"
    }

    fn tools(&self) -> &[&'static str] {
        &["python3", "python", "pip", "uv", "poetry"]
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        let pip = env.dir_or("PIP_CACHE_DIR", || env.cache_dir().join("pip"));
        let uv = env.dir_or("UV_CACHE_DIR", || env.xdg_cache_dir().join("uv"));
        let pre_commit = env.dir_or("PRE_COMMIT_HOME", || env.xdg_cache_dir().join("pre-commit"));
        // Default only: POETRY_CACHE_DIR's `cache`/`artifacts` subdirs are generic names.
        let poetry = env.cache_dir().join("pypoetry");

        vec![
            GlobalSpec {
                verify: &["http-v2", "http", "wheels"],
                ..global("pip", "pip cache", pip, Safety::Redownload)
            },
            GlobalSpec {
                verify: TAGGED,
                ..global("uv", "uv cache", uv, Safety::Redownload)
            },
            global(
                "poetry-cache",
                "poetry cache",
                default(poetry.join("cache")),
                Safety::Redownload,
            ),
            global(
                "poetry-artifacts",
                "poetry artifacts",
                default(poetry.join("artifacts")),
                Safety::Redownload,
            ),
            GlobalSpec {
                verify: &["db.db"],
                ..global(
                    "pre-commit",
                    "pre-commit envs",
                    pre_commit,
                    Safety::Regenerable,
                )
            },
            // Default only, and Caution: models can be very large and slow to download again.
            global(
                "huggingface-hub",
                "hugging face models",
                default(env.xdg_cache_dir().join("huggingface/hub")),
                Safety::Caution,
            ),
        ]
    }

    fn project_rules(&self) -> &[ProjectRule] {
        PROJECT_RULES
    }
}
