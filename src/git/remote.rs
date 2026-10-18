//! Fetch, pull and push: the calls that reach another computer. They run until
//! they end, are cancelled or reach [`LIMIT`], and never ask for a password (see
//! [`Git::run_remote`]).

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::actions::{ActionError, output_of};
use super::run::{Git, Outcome, failed};

/// How long a remote command may take.
pub(crate) const LIMIT: Duration = Duration::from_secs(5 * 60);

/// What to add when the remote asked for a secret.
pub(crate) const AUTH_HINT: &str = "authenticate once in the terminal pane (run the same git command there), or set up a credential helper or an ssh agent";

impl Git {
    /// Fetches from the remote of the branch (git's own choice of remote).
    pub(crate) fn fetch(&self, root: &Path, cancel: &AtomicBool) -> Result<(), ActionError> {
        self.remote_call(root, "fetch", &["fetch"], cancel, LIMIT)
    }

    /// Brings the branch up to date with its upstream, only when that is a move
    /// forward: branches that have diverged fail with git's text and change nothing,
    /// so no merge or rebase is ever started from here.
    pub(crate) fn pull(&self, root: &Path, cancel: &AtomicBool) -> Result<(), ActionError> {
        self.remote_call(root, "pull", &["pull", "--ff-only"], cancel, LIMIT)
    }

    /// Pushes the branch. A branch with no upstream is pushed with `-u` to the one
    /// remote the repository has; with none or several there is nothing to choose
    /// from here and it says so.
    pub(crate) fn push(&self, root: &Path, cancel: &AtomicBool) -> Result<(), ActionError> {
        self.push_within(root, cancel, LIMIT)
    }

    pub(super) fn push_within(
        &self,
        root: &Path,
        cancel: &AtomicBool,
        limit: Duration,
    ) -> Result<(), ActionError> {
        let upstream = self.run(
            root,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        )?;
        if upstream.success {
            return self.remote_call(root, "push", &["push"], cancel, limit);
        }
        let branch = self.run(root, &["symbolic-ref", "--short", "-q", "HEAD"])?;
        let branch = String::from_utf8_lossy(&branch.stdout).trim().to_owned();
        if branch.is_empty() {
            return Err(ActionError::BadArgument(
                "no branch is checked out, so there is nothing to push".to_owned(),
            ));
        }
        let remotes = self.run(root, &["remote"])?;
        let remotes: Vec<String> = String::from_utf8_lossy(&remotes.stdout)
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        match remotes.as_slice() {
            [] => Err(ActionError::BadArgument(
                "the repository has no remote to push to".to_owned(),
            )),
            [remote] => self.remote_call(
                root,
                "push",
                &["push", "-u", remote, &branch],
                cancel,
                limit,
            ),
            several => Err(ActionError::BadArgument(format!(
                "`{branch}` has no upstream and there are several remotes ({}): choose one in the terminal with `git push -u <remote> {branch}`",
                several.join(", ")
            ))),
        }
    }

    pub(super) fn remote_call(
        &self,
        root: &Path,
        name: &str,
        args: &[&str],
        cancel: &AtomicBool,
        limit: Duration,
    ) -> Result<(), ActionError> {
        let outcome = self.run_remote(root, args, cancel, limit)?;
        if outcome.success {
            return Ok(());
        }
        Err(remote_failure(name, &outcome))
    }
}

/// Why a remote command failed: the need for a secret has an error of its own.
fn remote_failure(name: &str, outcome: &Outcome) -> ActionError {
    let said = output_of(outcome);
    match needs_secret(&said) {
        true => ActionError::NeedsAuthentication(said),
        false => failed_action(name, outcome),
    }
}

fn failed_action(name: &str, outcome: &Outcome) -> ActionError {
    ActionError::Git(failed(name, output_of(outcome)))
}

/// Whether what git printed is the remote asking for something the editor cannot
/// give: a password it would have asked for, a key that is not there, a host it
/// does not know.
pub(super) fn needs_secret(said: &str) -> bool {
    let said = said.to_lowercase();
    [
        "terminal prompts disabled",
        "could not read username",
        "could not read password",
        "authentication failed",
        "permission denied (publickey",
        "host key verification failed",
        "no such device or address",
    ]
    .iter()
    .any(|sign| said.contains(sign))
}
