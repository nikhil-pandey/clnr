use clnr_core::{Env, GlobalSpec, Marker, ProjectRule, Provider, Safety};

use crate::common::{default, global};

const PROJECT_RULES: &[ProjectRule] = &[
    ProjectRule {
        id: "gradle-build",
        dir: "build",
        marker: Marker::AnyFile(&["build.gradle", "build.gradle.kts"]),
        verify: &["classes", "libs", "tmp", "intermediates", "generated"],
        safety: Safety::Regenerable,
    },
    ProjectRule {
        id: "maven-target",
        dir: "target",
        marker: Marker::File("pom.xml"),
        verify: &["classes", "test-classes", "maven-status", "maven-archiver"],
        safety: Safety::Regenerable,
    },
];

/// Gradle and Maven.
pub struct Jvm;

impl Provider for Jvm {
    fn id(&self) -> &'static str {
        "jvm"
    }

    fn name(&self) -> &'static str {
        "Gradle / Maven"
    }

    fn tools(&self) -> &[&'static str] {
        &["gradle", "mvn", "java"]
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        let (gradle, source) = env.dir_or("GRADLE_USER_HOME", || env.home().join(".gradle"));
        vec![
            GlobalSpec {
                verify: &["modules-2"],
                ..global(
                    "gradle-caches",
                    "gradle caches",
                    (gradle.join("caches"), source.clone()),
                    Safety::Redownload,
                )
            },
            global(
                "gradle-wrapper",
                "gradle wrapper dists",
                (gradle.join("wrapper/dists"), source),
                Safety::Redownload,
            ),
            // Default only: `localRepository` in settings.xml is not read.
            global(
                "maven-repo",
                "maven repository",
                default(env.home().join(".m2/repository")),
                Safety::Redownload,
            ),
        ]
    }

    fn project_rules(&self) -> &[ProjectRule] {
        PROJECT_RULES
    }
}
