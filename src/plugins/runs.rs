//! What decides whether a plugin may run a command (`process.run`), and which commands
//! it has running: its allowlist, whether the open folder is trusted, the folder
//! itself, and a way to stop each command. Nothing here runs anything (see `exec`).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use super::exec::{Control, Spec};
use crate::plugin_api::{MAX_OPEN_RUNS, Refusal, RunPattern, allows};

#[derive(Debug, Default)]
pub(super) struct Runs {
    /// The project folder, and whether the user trusts it.
    root: Option<PathBuf>,
    trusted: bool,
    /// The plugins that may run commands, with the commands each may run.
    allowed: HashMap<Box<str>, Vec<RunPattern>>,
    /// The commands that run: plugin, then the plugin's request number.
    open: HashMap<Box<str>, HashMap<u64, Arc<Control>>>,
}

impl Runs {
    /// The folder commands run in, and whether the user trusts it. What is returned
    /// is the plugins to tell, when trust is not what it was.
    pub(super) fn set_workspace(&mut self, root: PathBuf, trusted: bool) -> Vec<Box<str>> {
        let changed = self.trusted != trusted;
        self.root = Some(root);
        self.trusted = trusted;
        match changed {
            true => self.allowed.keys().cloned().collect(),
            false => Vec::new(),
        }
    }

    pub(super) fn trusted(&self) -> bool {
        self.trusted
    }

    /// `plugin` may run what `patterns` allow.
    pub(super) fn allow(&mut self, plugin: &str, patterns: Vec<RunPattern>) {
        self.allowed.insert(plugin.into(), patterns);
    }

    /// Whether the command may run, and what to run: the program is allowed, the folder
    /// is trusted, the working folder is inside the project, and the plugin has room.
    pub(super) fn check(
        &self,
        plugin: &str,
        request: u64,
        program: String,
        args: Vec<String>,
        cwd: &str,
    ) -> Result<Spec, Refusal> {
        let patterns = self.allowed.get(plugin).map_or(&[][..], Vec::as_slice);
        if !allows(patterns, &program, &args) {
            return Err(Refusal::NotAllowed {
                detail: format!(
                    "no `run` entry of its manifest allows `{}`",
                    show(&program, &args)
                ),
            });
        }
        if !self.trusted {
            return Err(Refusal::Untrusted);
        }
        let Some(root) = &self.root else {
            return Err(Refusal::Failed {
                detail: "there is no project folder".to_owned(),
            });
        };
        let opened = self.open.get(plugin);
        if opened.is_some_and(|runs| runs.contains_key(&request)) {
            return Err(Refusal::BadArgument {
                detail: format!("the command {request} is running already"),
            });
        }
        if opened.is_some_and(|runs| runs.len() >= MAX_OPEN_RUNS) {
            return Err(Refusal::Busy);
        }
        let cwd = folder_in(root, cwd).map_err(|detail| Refusal::BadArgument { detail })?;
        Ok(Spec { program, args, cwd })
    }

    /// The command is running. `false` when the plugin is not one that may run commands
    /// (it was turned off while the command was being started): the command is not kept.
    pub(super) fn begin(&mut self, plugin: &str, request: u64, control: Arc<Control>) -> bool {
        if !self.allowed.contains_key(plugin) {
            control.lose();
            return false;
        }
        self.open
            .entry(plugin.into())
            .or_default()
            .insert(request, control);
        true
    }

    /// The command ended.
    pub(super) fn finish(&mut self, plugin: &str, request: u64) {
        let Some(runs) = self.open.get_mut(plugin) else {
            return;
        };
        runs.remove(&request);
        if runs.is_empty() {
            self.open.remove(plugin);
        }
    }

    /// The plugin gives the command up.
    pub(super) fn cancel(&self, plugin: &str, request: u64) {
        if let Some(control) = self.open.get(plugin).and_then(|runs| runs.get(&request)) {
            control.cancel();
        }
    }

    /// The plugin starts again from nothing: what it had running is killed, and nobody
    /// is told.
    pub(super) fn lose(&mut self, plugin: &str) {
        for control in self
            .open
            .remove(plugin)
            .into_iter()
            .flat_map(HashMap::into_values)
        {
            control.lose();
        }
    }

    /// The plugin is gone: [`Runs::lose`], and it may run nothing more.
    pub(super) fn forget(&mut self, plugin: &str) {
        self.lose(plugin);
        self.allowed.remove(plugin);
    }
}

fn show(program: &str, args: &[String]) -> String {
    let mut text = program.to_owned();
    for arg in args {
        text.push(' ');
        text.push_str(arg);
    }
    text
}

/// The folder `cwd` names (from the root of the project, with `/`; the root when empty),
/// when it is a folder inside the project.
fn folder_in(root: &Path, cwd: &str) -> Result<PathBuf, String> {
    let relative = Path::new(cwd);
    if !relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(format!("`{cwd}` is not a folder inside the project"));
    }
    let wanted = root.join(relative);
    let (Ok(root), Ok(folder)) = (root.canonicalize(), wanted.canonicalize()) else {
        return Err(format!("`{cwd}` is not a folder of the project"));
    };
    match folder.starts_with(&root) && folder.is_dir() {
        true => Ok(folder),
        false => Err(format!("`{cwd}` is not a folder inside the project")),
    }
}

#[cfg(test)]
mod tests;
