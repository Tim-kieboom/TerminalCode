//! The row at the top of the screen with the menu icon: a click on the icon, or
//! `f10`, opens the main menu. The row itself takes no keyboard.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Paragraph};

use crate::app::state::AppState;
use crate::ui::Render;
use crate::ui::layout::Placement;

/// What the icon is drawn as: three lines, which every terminal font has (a gear
/// needs a font that has it).
const ICON: &str = " ≡ ";

#[derive(Debug, Default)]
pub struct MenuBar;

impl MenuBar {
    /// Where a click opens the menu: the icon, at the left end of the row.
    pub(crate) fn icon_rect(area: Rect) -> Rect {
        Rect {
            width: u16::try_from(ICON.chars().count())
                .unwrap_or(u16::MAX)
                .min(area.width),
            height: area.height.min(1),
            ..area
        }
    }
}

impl Render for MenuBar {
    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        let theme = state.theme();
        frame.render_widget(Block::new().style(theme.style("menubar")), placement.area);
        let icon = Self::icon_rect(placement.area);
        frame.render_widget(
            Paragraph::new(ICON).style(theme.style("menubar").patch(theme.style("menubar.icon"))),
            icon,
        );
    }
}
