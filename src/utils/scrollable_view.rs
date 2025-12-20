use ratatui::layout::Rect;

use crate::utils::cursor::Cursor;

#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollableView {
    height: usize,
    width: usize,
    viewport_top: usize,
    viewport_left: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct CursorUsize {
    line: usize,
    offset: usize,
}

impl ScrollableView {
    pub fn from_area(area: Rect, header_height: u16) -> Self {
        let mut this = Self {
            height: 0,
            width: 0,
            viewport_left: 0,
            viewport_top: 0,
        };
        this.update_area(area, header_height);
        this
    }

    pub fn update_area(&mut self, new_area: Rect, header_height: u16) {
        self.height = new_area.height.saturating_sub(header_height) as usize;
        self.width = new_area.width as usize;

        let max_top = (u16::MAX as usize)
            .saturating_sub(self.height)
            .min(u16::MAX as usize);
        self.viewport_top = self.viewport_top.min(max_top);

        let max_left = (u16::MAX as usize)
            .saturating_sub(self.width)
            .min(u16::MAX as usize);
        self.viewport_left = self.viewport_left.min(max_left);
    }

    pub fn text_buffer_to_view(&mut self, cursor: &Cursor, buffer: &[String]) -> String {
        self.scroll_to_cursor(to_cursor_usize(cursor), buffer);

        let max_view_height = self.height;
        let actual_viewport_top = self
            .viewport_top
            .min(buffer.len().saturating_sub(max_view_height));

        let mut text = String::new();
        for i in 0..max_view_height {
            let buffer_line_idx = actual_viewport_top + i;
            if buffer_line_idx >= buffer.len() {
                break;
            }

            if let Some(slice) = self.line_slice(&buffer[buffer_line_idx]) {
                text.push_str(slice);
            }
            text.push('\n');
        }
        text.trim_end_matches('\n').to_string()
    }

    fn line_slice<'a>(&self, line: &'a str) -> Option<&'a str> {
        let start = self.viewport_left.min(line.len());
        let end = (start + self.width).min(line.len());
        if start < end {
            Some(&line[start..end])
        } else {
            None
        }
    }

    fn scroll_to_cursor(&mut self, cursor: CursorUsize, buffer: &[String]) {
        self.scroll_vertical(cursor.line, buffer.len(), self.height);
        self.scroll_horizontal(cursor.offset, &buffer[cursor.line], self.width);
    }

    fn scroll_vertical(&mut self, cursor_line: usize, buffer_height: usize, view_height: usize) {
        let should_scroll_up = cursor_line < self.viewport_top;
        let should_scroll_down = cursor_line >= self.viewport_top + view_height;
        let should_center = cursor_line >= self.viewport_top + view_height / 2;

        if should_scroll_up {
            self.viewport_top = cursor_line;
        } else if should_scroll_down {
            self.viewport_top = cursor_line.saturating_sub(view_height.saturating_sub(1));
        } else if should_center {
            let target_top = cursor_line.saturating_sub(view_height / 3);
            self.viewport_top = self.viewport_top.max(target_top);
        }

        let buffer_bounds = buffer_height.saturating_sub(view_height).max(0);
        self.viewport_top = self.viewport_top.min(buffer_bounds);
    }

    fn scroll_horizontal(&mut self, cursor_offset: usize, cursor_line: &str, view_width: usize) {
        let should_scroll_left = cursor_offset < self.viewport_left;
        let should_scroll_right = cursor_offset >= self.viewport_left + view_width;

        if should_scroll_left {
            self.viewport_left = cursor_offset;
        } else if should_scroll_right {
            self.viewport_left = cursor_offset.saturating_sub(view_width.saturating_sub(1));
        }

        let line_bounds = cursor_line.len().saturating_sub(view_width).max(0);
        self.viewport_left = self.viewport_left.min(line_bounds);
    }
}

fn to_cursor_usize(this: &Cursor) -> CursorUsize {
    CursorUsize {
        line: this.line as usize,
        offset: this.offset as usize,
    }
}
