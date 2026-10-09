use ratatui::Frame;

use crate::state::AppState;
use crate::ui::layout::Placement;

pub mod fuzzy;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HideableKind {
    Show,
    Hide,
}

#[derive(Debug, Clone)]
pub struct Hideable<T: Render> {
    pub node: T,
    kind: HideableKind,
}
impl<T: Render + Default> Default for Hideable<T> {
    fn default() -> Self {
        Self::new_hide(T::default())
    }
}
impl<T: Render + Default> Hideable<T> {
    /// Hides it and hands back what was shown, leaving a default behind; `None`
    /// if it was hidden already. For code that changes the value while it also
    /// needs the rest of the state, and puts it back with `new_show`.
    pub fn take(&mut self) -> Option<T> {
        match self.kind {
            HideableKind::Show => {
                self.kind = HideableKind::Hide;
                Some(std::mem::take(&mut self.node))
            }
            HideableKind::Hide => None,
        }
    }
}
impl<T: Render> Hideable<T> {
    pub fn new_show(node: T) -> Self {
        Self {
            node,
            kind: HideableKind::Show,
        }
    }

    pub fn new_hide(node: T) -> Self {
        Self {
            node,
            kind: HideableKind::Hide,
        }
    }

    pub fn set_kind(&mut self, kind: HideableKind) {
        self.kind = kind
    }

    #[cfg(test)]
    pub fn hideable_kind(&self) -> HideableKind {
        self.kind
    }

    pub fn is_shown(&self) -> bool {
        self.kind == HideableKind::Show
    }

    /// Shows it if it is hidden and hides it if it is shown.
    pub fn toggle(&mut self) {
        self.kind = match self.kind {
            HideableKind::Show => HideableKind::Hide,
            HideableKind::Hide => HideableKind::Show,
        };
    }

    pub fn try_get(&self) -> Option<&T> {
        match self.kind {
            HideableKind::Show => Some(&self.node),
            HideableKind::Hide => None,
        }
    }

    pub fn try_get_mut(&mut self) -> Option<&mut T> {
        match self.kind {
            HideableKind::Show => Some(&mut self.node),
            HideableKind::Hide => None,
        }
    }
}
