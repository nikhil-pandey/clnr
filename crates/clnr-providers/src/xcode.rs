use std::path::{Path, PathBuf};

use clnr_core::{Env, GlobalSpec, Os, Provider, Safety};

use crate::common::{default, global, subdirs};

const MODULE_CACHE: &str = "ModuleCache.noindex";
const DEVICE_PLATFORMS: &[&str] = &["iOS", "watchOS", "tvOS", "visionOS"];

/// Xcode DerivedData, archives, device support files, and caches (macOS only).
///
/// A custom DerivedData location set in Xcode's preferences is not read.
pub struct Xcode;

impl Provider for Xcode {
    fn id(&self) -> &'static str {
        "xcode"
    }

    fn name(&self) -> &'static str {
        "Xcode"
    }

    fn tools(&self) -> &[&'static str] {
        &["xcodebuild"]
    }

    fn globals(&self, env: &Env) -> Vec<GlobalSpec> {
        if env.os() != Os::MacOs {
            return vec![];
        }
        let developer = env.home().join("Library/Developer");
        let xcode = developer.join("Xcode");
        let derived = xcode.join("DerivedData");

        // One entry per project (`Name-hash`), owned by the workspace recorded in its info.plist.
        let mut out: Vec<GlobalSpec> = subdirs(&derived)
            .filter(|p| !p.ends_with(MODULE_CACHE))
            .map(|entry| {
                // Shown by its workspace when known; otherwise by the project name.
                let owner = workspace_of(&entry);
                let label = match owner {
                    Some(_) => "DerivedData".to_owned(),
                    None => format!("DerivedData: {}", project_name(&entry)),
                };
                GlobalSpec {
                    verify: &["info.plist"],
                    owner,
                    ..global("derived-data", &label, default(entry), Safety::Regenerable)
                }
            })
            .collect();

        out.push(global(
            "module-cache",
            "module cache",
            default(derived.join(MODULE_CACHE)),
            Safety::Regenerable,
        ));
        // Needed to symbolicate or re-export shipped builds.
        out.push(global(
            "archives",
            "archives",
            default(xcode.join("Archives")),
            Safety::Caution,
        ));
        // Copied again from a device when it is next connected.
        for platform in DEVICE_PLATFORMS {
            for version in subdirs(&xcode.join(format!("{platform} DeviceSupport"))) {
                let label = format!(
                    "{platform} device support {}",
                    version.file_name().unwrap_or_default().to_string_lossy()
                );
                out.push(global(
                    "device-support",
                    &label,
                    default(version),
                    Safety::Redownload,
                ));
            }
        }
        out.push(global(
            "xcode-cache",
            "xcode cache",
            default(env.cache_dir().join("com.apple.dt.Xcode")),
            Safety::Regenerable,
        ));
        out.push(global(
            "simulator-caches",
            "simulator caches",
            default(developer.join("CoreSimulator/Caches")),
            Safety::Regenerable,
        ));
        out
    }
}

/// `MyApp-abcdefgh...` → `MyApp`.
fn project_name(entry: &Path) -> String {
    let name = entry.file_name().unwrap_or_default().to_string_lossy();
    match name.rsplit_once('-') {
        Some((project, _hash)) => project.to_owned(),
        None => name.into_owned(),
    }
}

/// The workspace or project a DerivedData entry was built from.
fn workspace_of(entry: &Path) -> Option<PathBuf> {
    let plist = plist::Value::from_file(entry.join("info.plist")).ok()?;
    let path = PathBuf::from(plist.as_dictionary()?.get("WorkspacePath")?.as_string()?);
    path.is_absolute().then_some(path)
}
