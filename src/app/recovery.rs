//! A terminal that goes away under the editor: what is unsaved is kept in recovery files
//! (see `persist::recovery`) and offered back at the next start in the same project.

use std::io;

use tokio::sync::mpsc;

use super::{App, Flow};
use crate::components::popups::confirm::Confirm;
use crate::components::workspace::Opened;
use crate::event::Event;
use crate::files::paths::canonical;
use crate::persist::recovery::{self, Found};

impl App {
    /// The terminal is gone (the window was closed, or the process was told to end): keeps
    /// every unsaved document in a recovery file and ends the loop without asking
    /// anything, for there is nobody to ask.
    pub(super) fn hang_up(&mut self) {
        self.flow = Flow::Quit;
        self.save_session();
        let Some(dir) = self.config_dir.as_deref() else {
            return;
        };
        let project = canonical(&self.state.project_root());
        let unsaved = self.state.workspace().unsaved_for_recovery(&project);
        if unsaved.is_empty() {
            return;
        }
        // Nobody is left to tell when it fails.
        let _ = recovery::write(dir, &unsaved);
    }

    /// Asks whether to bring back what an earlier hangup kept for this project folder.
    /// Files that cannot be read are reported and left alone.
    pub(crate) fn offer_recovery(&mut self) {
        let Some(found) = self.recovery_files() else {
            return;
        };
        if found.is_empty() {
            return;
        }
        let names: Vec<String> = found.iter().map(|f| f.recovery.name()).collect();
        self.state.open_confirm(Confirm::recover(&names));
        self.needs_redraw = true;
    }

    /// The user said yes: opens every kept text and removes the files.
    pub(in crate::app) fn recover_files(&mut self) {
        let Some(found) = self.recovery_files() else {
            return;
        };
        let mut copies = Vec::new();
        for item in &found {
            if self.state.workspace_mut().open_recovered(&item.recovery) == Opened::AsCopy
                && item.recovery.file.is_some()
            {
                copies.push(item.recovery.name());
            }
        }
        self.forget_recovery_files(&found);
        let names: Vec<String> = found.iter().map(|f| f.recovery.name()).collect();
        self.state.notify(format!("recovered {}", names.join(", ")));
        if !copies.is_empty() {
            self.state.notify_error(format!(
                "{} changed on disk since it was kept, so the recovered text is in a tab named \"recovered: ...\" that you can save as you like",
                copies.join(", ")
            ));
        }
        self.needs_redraw = true;
    }

    /// The user said no: removes the files.
    pub(in crate::app) fn discard_recovery_files(&mut self) {
        let Some(found) = self.recovery_files() else {
            return;
        };
        self.forget_recovery_files(&found);
        self.needs_redraw = true;
    }

    fn forget_recovery_files(&mut self, found: &[Found]) {
        let paths: Vec<_> = found.iter().map(|f| f.path.clone()).collect();
        if let Err(error) = recovery::remove(&paths) {
            self.state
                .notify_error(format!("could not remove the recovery files: {error}"));
        }
    }

    /// The recovery files of this project folder; `None` without a config folder. What
    /// cannot be read is reported.
    fn recovery_files(&mut self) -> Option<Vec<Found>> {
        let dir = self.config_dir.as_deref()?;
        let project = canonical(&self.state.project_root());
        let search = recovery::find(dir, &project);
        for problem in search.problems {
            self.state.notify_error(format!("recovery file: {problem}"));
        }
        Some(search.found)
    }
}

/// Waits for the signals that say the terminal is gone or the editor is to end: the
/// hangup of a closed window and the termination request on Unix, the closing of the
/// console on Windows.
pub(super) struct Hangup {
    #[cfg(unix)]
    hangup: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(windows)]
    close: tokio::signal::windows::CtrlClose,
}

impl Hangup {
    /// Starts listening; the default action of the signals (ending the process at once)
    /// is replaced from here on.
    pub(super) fn listen() -> io::Result<Self> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            Ok(Self {
                hangup: signal(SignalKind::hangup())?,
                terminate: signal(SignalKind::terminate())?,
            })
        }
        #[cfg(windows)]
        {
            Ok(Self {
                close: tokio::signal::windows::ctrl_close()?,
            })
        }
    }

    async fn wait(&mut self) {
        #[cfg(unix)]
        tokio::select! {
            _ = self.hangup.recv() => {}
            _ = self.terminate.recv() => {}
        }
        #[cfg(windows)]
        let _ = self.close.recv().await;
    }

    /// Sends [`Event::Hangup`] when a signal comes.
    pub(super) fn report_to(mut self, events: mpsc::Sender<Event>) {
        tokio::spawn(async move {
            self.wait().await;
            let _ = events.send(Event::Hangup).await;
        });
    }
}
