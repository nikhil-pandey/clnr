use std::collections::HashSet;
use std::path::Path;
use std::time::SystemTime;

use clnr_core::{Artifact, Env, Location, Origin, Provider, Safety, Size, human};
use serde::Deserialize;

use crate::common::run;

const DANGLING: &str = "dangling-images";
const IMAGE: &str = "image";
const CONTAINER: &str = "container";
const BUILD_CACHE: &str = "build-cache";

/// Docker images, stopped containers, and build cache, through the `docker` CLI (so contexts,
/// Docker Desktop, Colima, and OrbStack all work). Volumes are not touched.
pub struct Docker;

impl Provider for Docker {
    fn id(&self) -> &'static str {
        "docker"
    }

    fn name(&self) -> &'static str {
        "Docker"
    }

    fn tools(&self) -> &[&'static str] {
        &["docker"]
    }

    fn discover(&self, env: &Env) -> Result<Vec<Artifact>, String> {
        let Some(docker) = env.which("docker") else {
            return Ok(vec![]);
        };
        let cli = Cli(&docker);

        let containers: Vec<ContainerInfo> = cli.inspect(
            &["container", "inspect", "--size"],
            &cli.ids(&["ps", "-a"])?,
        )?;
        let used: HashSet<&str> = containers.iter().map(|c| c.image.as_str()).collect();
        let images: Vec<ImageInfo> =
            cli.inspect(&["image", "inspect"], &cli.ids(&["image", "ls"])?)?;

        let unused = |i: &&ImageInfo| !used.contains(i.id.as_str());
        let (dangling, tagged): (Vec<&ImageInfo>, Vec<&ImageInfo>) = images
            .iter()
            .filter(unused)
            .partition(|i| i.tags().is_empty());

        let mut out: Vec<Artifact> = Vec::new();
        if !dangling.is_empty() {
            out.push(
                Object {
                    rule: DANGLING,
                    label: "dangling images",
                    name: format!("{} untagged", dangling.len()),
                    ids: dangling.iter().map(|i| i.id.clone()).collect(),
                    safety: Safety::Regenerable,
                    bytes: dangling.iter().map(|i| i.size).sum(),
                    created: dangling.iter().filter_map(|i| parse_time(&i.created)).max(),
                }
                .into(),
            );
        }
        out.extend(tagged.iter().map(|image| {
            Artifact::from(Object {
                rule: IMAGE,
                label: "image",
                name: image.tags().join(", "),
                ids: vec![image.id.clone()],
                safety: Safety::Redownload,
                bytes: image.size,
                created: parse_time(&image.created),
            })
        }));
        out.extend(
            containers
                .iter()
                .filter(|c| c.is_stopped())
                .map(|container| {
                    Artifact::from(Object {
                        rule: CONTAINER,
                        label: "container",
                        name: container.name.trim_start_matches('/').to_owned(),
                        ids: vec![container.id.clone()],
                        safety: Safety::Caution,
                        bytes: container.size_rw.unwrap_or(0),
                        created: parse_time(&container.created),
                    })
                }),
        );
        if let Some(bytes) = cli.build_cache_reclaimable()?
            && bytes > 0
        {
            out.push(
                Object {
                    rule: BUILD_CACHE,
                    label: "build cache",
                    name: "unused build cache".to_owned(),
                    ids: vec![],
                    safety: Safety::Regenerable,
                    bytes,
                    created: None,
                }
                .into(),
            );
        }
        Ok(out)
    }

    fn delete(&self, artifact: &Artifact, env: &Env) -> Result<(), String> {
        let docker = env.which("docker").ok_or("docker is not on PATH")?;
        let Location::Managed { ids, .. } = &artifact.location else {
            return Err("not a docker artifact".to_owned());
        };
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        // No `--force`: Docker refuses to remove anything that came into use since the scan.
        let args: Vec<&str> = match artifact.rule {
            DANGLING | IMAGE => [&["image", "rm"][..], &ids].concat(),
            CONTAINER => [&["container", "rm"][..], &ids].concat(),
            BUILD_CACHE => vec!["builder", "prune", "--all", "--force"],
            other => return Err(format!("unknown docker rule `{other}`")),
        };
        run(&docker, &args).map(drop)
    }
}

struct Cli<'a>(&'a Path);

impl Cli<'_> {
    /// Full ids from a listing command (`ps -a`, `image ls`), deduplicated.
    fn ids(&self, list: &[&str]) -> Result<Vec<String>, String> {
        let args = [list, &["--quiet", "--no-trunc"]].concat();
        let mut ids: Vec<String> = run(self.0, &args)?.lines().map(str::to_owned).collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// `docker ... inspect` output for `ids`, parsed.
    fn inspect<T: for<'de> Deserialize<'de>>(
        &self,
        cmd: &[&str],
        ids: &[String],
    ) -> Result<Vec<T>, String> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let args: Vec<&str> = cmd
            .iter()
            .copied()
            .chain(ids.iter().map(String::as_str))
            .collect();
        serde_json::from_str(&run(self.0, &args)?)
            .map_err(|e| format!("unexpected docker output: {e}"))
    }

    /// Reclaimable build cache bytes from `docker system df`.
    fn build_cache_reclaimable(&self) -> Result<Option<u64>, String> {
        let out = run(self.0, &["system", "df", "--format", "{{json .}}"])?;
        Ok(out
            .lines()
            .filter_map(|l| serde_json::from_str::<DfRow>(l).ok())
            .find(|row| row.kind == "Build Cache")
            // "14.33GB" or "1.2GB (50%)"
            .and_then(|row| row.reclaimable.split_whitespace().next().map(str::to_owned))
            .and_then(|s| human::parse_size(&s).ok()))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageInfo {
    id: String,
    repo_tags: Option<Vec<String>>,
    size: u64,
    created: String,
}

impl ImageInfo {
    fn tags(&self) -> &[String] {
        self.repo_tags.as_deref().unwrap_or_default()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerInfo {
    id: String,
    /// The image id (`sha256:...`).
    image: String,
    name: String,
    created: String,
    state: ContainerState,
    size_rw: Option<u64>,
}

impl ContainerInfo {
    fn is_stopped(&self) -> bool {
        matches!(self.state.status.as_str(), "exited" | "created" | "dead")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerState {
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DfRow {
    #[serde(rename = "Type")]
    kind: String,
    reclaimable: String,
}

fn parse_time(s: &str) -> Option<SystemTime> {
    let ts: jiff::Timestamp = s.parse().ok()?;
    SystemTime::try_from(ts).ok()
}

/// A Docker object (or group of them) to offer as one artifact.
struct Object {
    rule: &'static str,
    /// The kind: `image`, `container`, ...
    label: &'static str,
    name: String,
    ids: Vec<String>,
    safety: Safety,
    bytes: u64,
    created: Option<SystemTime>,
}

impl From<Object> for Artifact {
    fn from(o: Object) -> Self {
        let location = Location::Managed {
            ids: o.ids,
            name: o.name,
        };
        let mut artifact = Artifact::new(
            "docker",
            o.rule,
            o.label,
            Origin::Global,
            location,
            o.safety,
        );
        artifact.size = Some(Size {
            on_disk: o.bytes,
            apparent: o.bytes,
            entries: 1,
        });
        artifact.newest_mtime = o.created;
        artifact
    }
}
