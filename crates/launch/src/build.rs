//! Building a target and finding the program it made.
//!
//! The build is `cargo build --message-format=json-render-diagnostics`: the errors are
//! written as cargo writes them (on its standard error, for the user to read), and what goes
//! to the standard output is JSON, one message a line, among which `compiler-artifact` names
//! the executable. [`ArtifactFinder`] reads those lines as they come.

use std::path::PathBuf;

use serde::Deserialize;

use crate::target::{Target, TargetId};

/// The arguments of `cargo` that build `target` (run them in the project). The path to
/// the package's `Cargo.toml` names the package, so two packages with a binary of one name are
/// told apart.
pub fn build_args(target: &Target, release: bool) -> Vec<String> {
    let id = target.id();
    let mut args = vec![
        "build".to_owned(),
        "--message-format=json-render-diagnostics".to_owned(),
        "--color".to_owned(),
        "always".to_owned(),
        "--manifest-path".to_owned(),
        target.manifest().display().to_string(),
        id.kind().flag().to_owned(),
        id.name().to_owned(),
    ];
    if release {
        args.push("--release".to_owned());
    }
    if !target.required_features().is_empty() {
        args.push("--features".to_owned());
        args.push(target.required_features().join(","));
    }
    args
}

/// The variables `cargo run` sets for the program that a program may read when it runs (not
/// the ones only the compiler sees): where its package is, and the package's name.
pub fn run_env(target: &Target) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(folder) = target.manifest().parent() {
        env.push((
            "CARGO_MANIFEST_DIR".to_owned(),
            folder.display().to_string(),
        ));
    }
    env.push((
        "CARGO_PKG_NAME".to_owned(),
        target.id().package().to_owned(),
    ));
    env
}

/// Finds the executable of one target in the standard output of a build.
#[derive(Debug)]
pub struct ArtifactFinder {
    wanted: TargetId,
    /// The start of a line whose end has not come yet.
    line: Vec<u8>,
    found: Option<PathBuf>,
}

impl ArtifactFinder {
    pub fn new(wanted: &TargetId) -> Self {
        Self {
            wanted: wanted.clone(),
            line: Vec::new(),
            found: None,
        }
    }

    /// Takes the next piece of what the build printed.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.line.extend_from_slice(bytes);
        while let Some(end) = self.line.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.line.drain(..=end).collect();
            self.read(&line);
        }
    }

    /// The executable, once the build is over: the last one the build said it made.
    pub fn finish(mut self) -> Option<PathBuf> {
        let rest = std::mem::take(&mut self.line);
        self.read(&rest);
        self.found
    }

    fn read(&mut self, line: &[u8]) {
        let Ok(message) = serde_json::from_slice::<Message>(line.trim_ascii()) else {
            return;
        };
        if message.reason != "compiler-artifact" {
            return;
        }
        let Some(target) = message.target else {
            return;
        };
        let kind = self.wanted.kind().as_str();
        let same = target.name == self.wanted.name() && target.kind.iter().any(|k| k == kind);
        if let (true, Some(executable)) = (same, message.executable) {
            self.found = Some(executable);
        }
    }
}

#[derive(Deserialize)]
struct Message {
    reason: String,
    target: Option<MessageTarget>,
    executable: Option<PathBuf>,
}

#[derive(Deserialize)]
struct MessageTarget {
    name: String,
    #[serde(default)]
    kind: Vec<String>,
}
