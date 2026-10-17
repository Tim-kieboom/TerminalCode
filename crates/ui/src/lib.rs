use ratatui::Frame;

use crate::layout::Placement;

pub mod backdrop;
pub mod bar;
mod component_kind;
pub mod cursor;
pub mod fuzzy;
pub mod hideable;
pub mod layout;
pub mod pane_frame;
#[cfg(test)]
mod tests;
pub mod theme;
pub mod view;

/// A pane drawn from the state `S` of the application (the editor's is `AppState`; the
/// state is a parameter so that this module does not depend on the app).
///
/// Drawing is split in two so that `render` can be read-only: `prepare` runs
/// first, with mutable access, and adjusts view state that depends on the
/// size of the pane (such as scroll). `self` is the component's own state;
/// `state` is the rest of the app (theme, status, ...).
pub trait Render<S> {
    /// Adjusts view state for the area the component will be drawn into.
    fn prepare(&mut self, _placement: &Placement) {}

    /// Draws the component. Must not depend on anything `prepare` did not set
    /// up, and cannot change state.
    fn render(&self, frame: &mut Frame, state: &S, placement: &Placement);
}

pub use component_kind::{ComponentKind, PluginViewId};
pub use hideable::Hideable;
