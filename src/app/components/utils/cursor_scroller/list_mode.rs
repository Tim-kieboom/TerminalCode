use crate::app::components::utils::cursor_scroller::{CursorScroller, HeightScroll, Position};

impl CursorScroller {
    pub(super) fn scroll_list(&mut self, cursor_visual_line: u16, height: u16) -> Position<u16> {
        let margin = 3;
        match self.direction.height {
            HeightScroll::Up => {
                if cursor_visual_line < self.vertical_offset + margin {
                    self.vertical_offset = cursor_visual_line.saturating_sub(margin);
                }
            }
            HeightScroll::Down => {
                if cursor_visual_line + margin >= self.vertical_offset + height {
                    self.vertical_offset = cursor_visual_line + 1 + margin - height;
                }
            }
        }

        Position {
            horizontal: 0,
            vertical: self.vertical_offset,
        }
    }
}
