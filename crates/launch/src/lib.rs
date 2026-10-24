//! The run configurations: what the Run key can start. The runnable targets of a cargo
//! project come from `cargo metadata` ([`Targets`]); a file in the project
//! (`.trcode/launch.toml`, [`LaunchFile`]) overrides them and adds commands of its own; and
//! [`RunList`] puts the two together into the list the picker shows.
//!
//! This crate only works out the list. Starting a build or a program is the editor's.
//! See "Run configurations and a debugger" in `docs/design.md`.

mod build;
mod file;
mod list;
mod target;

pub use build::{ArtifactFinder, build_args, run_env};
pub use file::{FILE, LaunchFile, Problem};
pub use list::{ConfigId, RunConfig, RunList, What};
pub use target::{IdError, Kind, MetadataError, Target, TargetId, Targets};

#[cfg(test)]
mod tests;
