//! clnr engine: the model, the provider trait, scanning, measuring, and deleting.
//!
//! No UI, no argument parsing, and no product defaults (such as which roots to scan) live here.

#![forbid(unsafe_code)]

pub mod clean;
pub mod config;
pub mod env;
pub mod filter;
pub mod human;
pub mod model;
pub mod provider;
pub mod registry;
pub mod scan;

pub use env::{Env, Os, Source};
pub use filter::Filter;
pub use model::{Artifact, ArtifactId, Location, Origin, Safety, Size};
pub use provider::{Detection, GlobalSpec, Marker, ProjectRule, Provider};
pub use registry::Registry;
