use ratatui::Frame;

use crate::state::AppState;
use crate::ui::layout::Placement;

pub mod fuzzy;
pub(crate) mod hideable;
pub mod layout;
pub mod pane_frame;
#[cfg(test)]
mod tests;
pub mod theme;
pub mod view;

/// A pane drawn from application state.
///
/// Drawing is split in two so that `render` can be read-only: `prepare` runs
/// first, with mutable access, and adjusts view state that depends on the
/// size of the pane (such as scroll). `self` is the component's own state;
/// `state` is the rest of the app (theme, status, ...).
pub trait Render {
    /// Adjusts view state for the area the component will be drawn into.
    fn prepare(&mut self, _placement: &Placement) {}

    /// Draws the component. Must not depend on anything `prepare` did not set
    /// up, and cannot change state.
    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement);
}

pub(crate) use hideable::Hideable;
