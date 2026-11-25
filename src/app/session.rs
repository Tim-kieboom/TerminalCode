//! The tabs and panes of a project folder, kept when the editor ends and brought back when
//! it starts there again (see `persist::session`, and the `restore_session` setting).

use super::{App, state::AppState};
use crate::components::popups::confirm::Confirm;
use crate::files::paths::canonical;
use crate::persist::session::{self, Session};
use crate::settings::RestoreSession;

/// How many files the question lists before it says "and N more".
const LISTED: usize = 8;

impl AppState {
    /// Brings `session` back and says which of its files were gone, in one message.
    pub(crate) fn restore_session(&mut self, session: &Session) {
        let skipped = self.workspace_mut().restore_session(session);
        if skipped.is_empty() {
            return;
        }
        let names: Vec<String> = skipped.iter().map(|path| name_of(path)).collect();
        self.notify(format!(
            "the last session's files that are gone were skipped: {}",
            names.join(", ")
        ));
    }
}

impl App {
    /// Keeps the tabs and panes of the project folder, or forgets them when none is left.
    /// Not while the question about bringing the last ones back is still open: what is
    /// on screen is not the session then.
    pub(crate) fn save_session(&mut self) {
        let Some(dir) = self.config_dir.as_deref() else {
            return;
        };
        if self.settings.restore_session == RestoreSession::Never || self.pending_session.is_some()
        {
            return;
        }
        let project = canonical(&self.state.project_root());
        let session = self.state.workspace().session(&project);
        // At the end of a run there is nobody to tell.
        let _ = session::save(dir, &project, session.as_ref());
    }

    /// Holds a session until the user has been asked about it (`"ask"`).
    pub(crate) fn with_pending_session(mut self, session: Option<Session>) -> Self {
        self.pending_session = session;
        self
    }

    /// The first question of a start: whether to bring the last session back, when the
    /// setting says to ask; otherwise the one about unsaved text that was kept.
    pub(crate) fn offer_session(&mut self) {
        let Some(session) = &self.pending_session else {
            self.offer_recovery();
            return;
        };
        let files = session.files();
        let mut names: Vec<String> = files
            .iter()
            .take(LISTED)
            .map(|path| name_of(path))
            .collect();
        if files.len() > LISTED {
            names.push(format!("and {} more", files.len() - LISTED));
        }
        self.state.open_confirm(Confirm::restore_session(&names));
        self.needs_redraw = true;
    }

    /// The answer to the question about the last session; either way the next question
    /// of the start follows.
    pub(in crate::app) fn answered_session(&mut self, restore: bool) {
        if let Some(session) = self.pending_session.take()
            && restore
        {
            self.state.restore_session(&session);
            self.needs_redraw = true;
        }
        self.offer_recovery();
    }
}

fn name_of(path: &std::path::Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}
