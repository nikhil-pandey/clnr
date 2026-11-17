//! Built-in providers, one module per tool family.
//!
//! Providers only describe where things live and how to recognize them; scanning, measuring,
//! and deleting happen in `clnr-core`.

#![forbid(unsafe_code)]

mod bazel;
mod cargo;
mod common;
mod docker;
mod dotnet;
mod go;
mod jvm;
mod node;
mod python;
mod swift;
mod terraform;
mod xcode;

pub use bazel::Bazel;
pub use cargo::Cargo;
pub use docker::Docker;
pub use dotnet::Dotnet;
pub use go::Go;
pub use jvm::Jvm;
pub use node::Node;
pub use python::Python;
pub use swift::Swift;
pub use terraform::Terraform;
pub use xcode::Xcode;

use clnr_core::Provider;

/// Every built-in provider, in display order.
pub fn builtin() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(Cargo),
        Box::new(Node),
        Box::new(Python),
        Box::new(Go),
        Box::new(Jvm),
        Box::new(Dotnet),
        Box::new(Terraform),
        Box::new(Bazel),
        Box::new(Docker),
        Box::new(Swift),
        Box::new(Xcode),
    ]
}
