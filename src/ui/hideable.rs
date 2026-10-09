//! Something on screen that can be hidden and shown again without losing its
//! state.

use ratatui::Frame;

use super::Render;
use crate::state::AppState;
use crate::ui::layout::Placement;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Visibility {
    Shown,
    Hidden,
}

/// A component that is either shown or hidden. Hiding keeps its state, so it
/// comes back as it was.
///
/// There is deliberately no `Default`: a component starts shown, and that is
/// spelled out with [`Hideable::new_show`]; [`Hideable::hide`] hides it.
#[derive(Debug, Clone)]
pub(crate) struct Hideable<T> {
    node: T,
    visibility: Visibility,
}

impl<T> Hideable<T> {
    pub(crate) fn new_show(node: T) -> Self {
        Self {
            node,
            visibility: Visibility::Shown,
        }
    }

    pub(crate) fn is_shown(&self) -> bool {
        self.visibility == Visibility::Shown
    }

    pub(crate) fn show(&mut self) {
        self.visibility = Visibility::Shown;
    }

    pub(crate) fn hide(&mut self) {
        self.visibility = Visibility::Hidden;
    }

    /// Shows it if it is hidden and hides it if it is shown.
    pub(crate) fn toggle(&mut self) {
        self.visibility = match self.visibility {
            Visibility::Shown => Visibility::Hidden,
            Visibility::Hidden => Visibility::Shown,
        };
    }

    /// The component, shown or not. Its state is kept while it is hidden, so
    /// this is how to read or change that state.
    pub(crate) fn node(&self) -> &T {
        &self.node
    }

    pub(crate) fn node_mut(&mut self) -> &mut T {
        &mut self.node
    }
}

/// A hidden component takes no part in drawing.
impl<T: Render> Render for Hideable<T> {
    fn prepare(&mut self, placement: &Placement) {
        if self.is_shown() {
            self.node.prepare(placement);
        }
    }

    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        if self.is_shown() {
            self.node.render(frame, state, placement);
        }
    }
}
