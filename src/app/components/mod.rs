use ratatui::{Frame, layout::Rect};

use crate::keybinds::PanelContext;
pub mod debug_window;
pub mod editor;
pub mod keybind_display;
pub mod notifications;
pub mod sidebar;
pub mod utils;

pub trait Component {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext);
}

pub struct Hideable<T: Component> {
    component: T,
    should_hide: bool,
}

impl<T: Component> Hideable<T> {
    pub fn new_hide(component: T) -> Self {
        Self {
            component,
            should_hide: true,
        }
    }

    pub fn new_show(component: T) -> Self {
        Self {
            component,
            should_hide: false,
        }
    }

    pub fn try_draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        if self.should_show() {
            self.component.draw(frame, area, context)
        }
    }

    pub fn should_hide(&self) -> bool {
        self.should_hide
    }

    pub fn should_show(&self) -> bool {
        !self.should_hide
    }

    pub fn toggle_hide(&mut self) {
        self.should_hide = !self.should_hide
    }

    pub fn hide(&mut self) {
        self.should_hide = true
    }

    pub fn inner(&self) -> &T {
        &self.component
    }

    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.component
    }
}
