use ratatui::Frame;

use crate::popups::lookup_bar::LookupBar;

pub mod lookup_bar;

#[derive(Debug, Clone)]
pub enum PopupKind {
    Empty,
    LookupBar(LookupBar),
}
impl PopupKind {
    
    pub fn is_empty(&self) -> bool {
        matches!(self, PopupKind::Empty)
    }

    pub fn draw_ui(&mut self, frame: &mut Frame) {

        match self {
            PopupKind::Empty => (),
            PopupKind::LookupBar(lookup_bar) => lookup_bar.draw_ui(frame),
        }
    }
}
impl Default for PopupKind {
    fn default() -> Self {
        Self::Empty
    }
}