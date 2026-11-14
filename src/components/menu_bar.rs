//! The row at the top of the screen with the menu icon: a click on the icon, or
//! `ctrl+k m`, opens the main menu. The row itself takes no keyboard.

use std::path::Path;

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::widgets::{Block, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::state::AppState;
use crate::ui::Render;
use crate::ui::layout::Placement;

/// What the icon is drawn as: three lines, which every terminal font has (a gear
/// needs a font that has it).
const ICON: &str = " ≡ ";

/// What the bar says for a tab that has no file yet.
const UNTITLED: &str = "[untitled]";

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

/// What the bar says of the open file: its path from the project folder, the whole path when
/// the file is outside it, `[untitled]` for a tab with no file, and nothing without a tab.
pub(crate) fn title_of(path: Option<&Path>, root: Option<&Path>, has_tab: bool) -> String {
    let Some(path) = path else {
        return if has_tab { UNTITLED } else { "" }.to_owned();
    };
    let whole = trcode_files::paths::without_verbatim_prefix(path);
    let inside = root.and_then(|root| {
        let root = trcode_files::paths::without_verbatim_prefix(root);
        whole.strip_prefix(root).ok().map(Path::to_path_buf)
    });
    inside.unwrap_or(whole).display().to_string()
}

/// `text` cut from the left, with `…` in front of what is left, until it is at most `width`
/// columns wide, so the end (the file name) stays. Empty when not even that fits.
pub(crate) fn cut_from_left(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let room = width.saturating_sub(1);
    let mut kept: Vec<char> = Vec::new();
    let mut used = 0;
    for c in text.chars().rev() {
        let columns = c.width().unwrap_or(0);
        if used + columns > room {
            break;
        }
        used += columns;
        kept.push(c);
    }
    if kept.is_empty() {
        return String::new();
    }
    kept.push('…');
    kept.iter().rev().collect()
}

impl Render<AppState> for MenuBar {
    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        let theme = state.theme();
        frame.render_widget(Block::new().style(theme.style("menubar")), placement.area);
        let icon = Self::icon_rect(placement.area);
        frame.render_widget(
            Paragraph::new(ICON).style(theme.style("menubar").patch(theme.style("menubar.icon"))),
            icon,
        );
        self.render_title(frame, state, placement.area);
    }
}

impl MenuBar {
    /// The path of the open file, in the middle of the row. It takes the room the icon's
    /// width leaves on both sides so it stays centered, and is cut from the left to fit.
    fn render_title(&self, frame: &mut Frame, state: &AppState, area: Rect) {
        let project = trcode_files::paths::without_verbatim_prefix(&state.project_root())
            .to_string_lossy()
            .to_string();

        let title = title_of(
            state.editor().buffer().path(),
            state.explorer().root_path(),
            state.workspace().has_tabs(),
        );

        let margin = Self::icon_rect(area).width;
        let room = usize::from(area.width.saturating_sub(margin * 2));
        let title_text = cut_from_left(&title, room);
        let project_text = cut_from_left(&project, 25);
        if title_text.is_empty() {
            return;
        }

        let theme = state.theme();
        let style = theme.style("menubar").patch(theme.style("menubar.path"));
        let inner = Rect {
            x: area.x + margin,
            width: area.width.saturating_sub(margin * 2),
            height: area.height.min(1),
            ..area
        };

        frame.render_widget(
            Paragraph::new(project_text)
                .style(style)
                .alignment(Alignment::Left),
            inner,
        );

        frame.render_widget(
            Paragraph::new(title_text)
                .style(style)
                .alignment(Alignment::Center),
            inner,
        );
    }
}

#[cfg(test)]
mod tests;
