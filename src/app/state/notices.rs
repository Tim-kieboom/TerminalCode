//! Messages shown to the user, and clicks on them.

use super::AppState;
use crate::components::notifications::{Hit, Level, Notifications};
use std::time::Instant;

impl AppState {
    /// Sets how long the info messages shown from now on stay.
    pub(crate) fn set_notification_lifetime(&mut self, lifetime: std::time::Duration) {
        self.notifications.set_lifetime(lifetime);
    }

    /// Shows an info message that goes away by itself.
    pub(crate) fn notify(&mut self, message: impl Into<Box<str>>) {
        self.notifications
            .push(Level::Info, message, Instant::now());
    }

    /// Shows an error that stays until the user presses a key or clicks.
    pub(crate) fn notify_error(&mut self, message: impl Into<Box<str>>) {
        self.notifications
            .push(Level::Error, message, Instant::now());
    }

    pub(crate) fn notifications(&self) -> &Notifications {
        &self.notifications
    }

    pub(crate) fn set_notification_hits(&mut self, hits: Vec<(Hit, ratatui::layout::Rect)>) {
        self.notification_hits = hits;
    }

    /// Removes what a click at a screen cell landed on, if it was on a
    /// notification as last drawn: the message, or the info messages a "+N more"
    /// line stands for. Returns whether the click was on one.
    pub(crate) fn click_notification(&mut self, column: u16, row: u16) -> bool {
        let position = ratatui::layout::Position::new(column, row);
        let Some((hit, _)) = self
            .notification_hits
            .iter()
            .find(|(_, area)| area.contains(position))
            .cloned()
        else {
            return false;
        };
        match hit {
            Hit::Message(id) => self.notifications.dismiss(id),
            Hit::More(shown) => self.notifications.dismiss_hidden_info(&shown),
        };
        true
    }

    /// The text of the newest message, if any.
    #[cfg(test)]
    pub(crate) fn latest_notification(&self) -> Option<&str> {
        self.notifications.latest().map(|item| &*item.text)
    }

    /// Removes errors the user has had a chance to see. Returns whether there
    /// were any.
    pub(crate) fn dismiss_errors(&mut self) -> bool {
        self.notifications.dismiss_errors()
    }

    /// Removes info messages that are over by `now`. Returns whether any went.
    pub(crate) fn expire_notifications(&mut self, now: Instant) -> bool {
        self.notifications.expire(now)
    }
}
