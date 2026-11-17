use std::cmp::Reverse;
use std::path::Path;
use std::time::SystemTime;

use anyhow::Result;
use clnr_core::{Artifact, Location, Origin, Safety, Size};
use serde::Serialize;

use crate::app::App;
use crate::args::ListArgs;
use crate::render;

pub fn run(app: &App, args: &ListArgs) -> Result<()> {
    let report = app.scan();
    render::scan_errors(&report.errors);

    let filter = args.filter.to_filter(&[]);
    let now = SystemTime::now();
    let mut artifacts: Vec<&Artifact> = report
        .artifacts
        .iter()
        .filter(|a| filter.matches(a, now))
        .collect();
    artifacts.sort_by_key(|a| Reverse(a.on_disk()));

    if args.json {
        print_json(&artifacts)
    } else {
        render::artifacts(artifacts, &app.env);
        Ok(())
    }
}

/// Stable machine-readable shape, independent of the core types.
#[derive(Serialize)]
struct JsonOut<'a> {
    version: u32,
    artifacts: Vec<JsonArtifact<'a>>,
}

#[derive(Serialize)]
struct JsonArtifact<'a> {
    id: String,
    provider: &'a str,
    rule: &'a str,
    label: &'a str,
    origin: &'a Origin,
    location: &'a Location,
    safety: Safety,
    size: Option<Size>,
    /// RFC 3339.
    newest_mtime: Option<String>,
    /// The project a global artifact belongs to.
    owner: Option<&'a Path>,
    orphaned: bool,
}

impl<'a> From<&'a Artifact> for JsonArtifact<'a> {
    fn from(a: &'a Artifact) -> Self {
        Self {
            id: a.id.to_string(),
            provider: a.provider,
            rule: a.rule,
            label: &a.label,
            origin: &a.origin,
            location: &a.location,
            safety: a.safety,
            size: a.size,
            newest_mtime: a
                .newest_mtime
                .and_then(|t| jiff::Timestamp::try_from(t).ok())
                .map(|t| t.to_string()),
            owner: a.owner.as_deref(),
            orphaned: a.orphaned,
        }
    }
}

fn print_json(artifacts: &[&Artifact]) -> Result<()> {
    let out = JsonOut {
        version: 1,
        artifacts: artifacts.iter().map(|&a| a.into()).collect(),
    };
    serde_json::to_writer_pretty(std::io::stdout().lock(), &out)?;
    println!();
    Ok(())
}
