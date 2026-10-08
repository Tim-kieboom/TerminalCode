use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Position as ScreenPosition, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::buffer::Selection;
use crate::components::editor::{
    Editor,
    text_layout::{self, digits, display_column, visible_cells},
};
use crate::state::AppState;
use crate::ui::layout::Placement;
use crate::ui::theme::Theme;
use crate::ui::{Render, get_block, pane_inner};

/// Smallest line-number column, in digits.
const MIN_GUTTER_DIGITS: usize = 3;

impl Render for Editor {
    /// Scrolls so the cursor stays visible in the area the editor will get.
    fn prepare(&mut self, placement: &Placement) {
        let inner = pane_inner(placement.area);
        let geometry = Geometry::new(inner, self.buffer().len_lines());
        keep_cursor_visible(self, &geometry);
    }

    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        let block = get_block(state, self.display_name());
        let inner = block.inner(placement.area);
        frame.render_widget(block, placement.area);

        let geometry = Geometry::new(inner, self.buffer().len_lines());
        draw_lines(frame, state.theme(), self, &geometry);
        place_cursor(frame, self, &geometry);
    }
}

fn keep_cursor_visible(editor: &mut Editor, geometry: &Geometry) {
    let head = editor.selections().primary().head();
    let Ok(text) = editor.buffer().line_content(head.line) else {
        return;
    };
    let column = display_column(&text, head.column);
    editor.scroll_to_show(
        head.line,
        column,
        usize::from(geometry.text.height),
        usize::from(geometry.text.width),
    );
}

/// Where the line numbers and the text go inside the editor pane.
struct Geometry {
    gutter: Rect,
    text: Rect,
}
impl Geometry {
    fn new(inner: Rect, line_count: usize) -> Self {
        let gutter_width = (digits(line_count).max(MIN_GUTTER_DIGITS) + 1) as u16;
        let gutter_width = gutter_width.min(inner.width);
        Self {
            gutter: Rect {
                width: gutter_width,
                ..inner
            },
            text: Rect {
                x: inner.x + gutter_width,
                width: inner.width - gutter_width,
                ..inner
            },
        }
    }
}

fn draw_lines(frame: &mut Frame, theme: &Theme, editor: &Editor, geometry: &Geometry) {
    let scroll = editor.scroll();
    let head_line = editor.selections().primary().head().line;
    let selection = editor.selections().primary();

    let mut numbers = Vec::new();
    let mut texts = Vec::new();
    let visible = scroll.top..scroll.top + usize::from(geometry.text.height);
    for line in visible.take_while(|line| *line < editor.buffer().len_lines()) {
        let number_slot = if line == head_line {
            "editor.line_number.current"
        } else {
            "editor.line_number"
        };
        let width = usize::from(geometry.gutter.width).saturating_sub(1);
        numbers.push(Line::styled(
            format!("{:>width$} ", line + 1),
            theme.style(number_slot),
        ));

        let Ok(content) = editor.buffer().line_content(line) else {
            continue;
        };
        let cells = visible_cells(&content, scroll.left, usize::from(geometry.text.width));
        let selected =
            selected_columns(selection, line, editor.buffer().line_len(line).unwrap_or(0));

        let selection_style = theme.style("editor.selection");
        let mut spans: Vec<Span> = cells
            .into_iter()
            .map(|cell| {
                let style = match &selected {
                    Some((range, _)) if range.contains(&cell.grapheme) => selection_style,
                    _ => Style::default(),
                };
                Span::styled(cell.text, style)
            })
            .collect();
        if matches!(selected, Some((_, true))) {
            spans.push(Span::styled(" ", selection_style));
        }
        texts.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(numbers), geometry.gutter);
    frame.render_widget(Paragraph::new(texts), geometry.text);
}

/// Grapheme columns of `line` covered by `selection`, and whether the
/// selection continues past the end of the line (so the line break shows as
/// selected).
fn selected_columns(
    selection: &Selection,
    line: usize,
    line_len: usize,
) -> Option<(Range<usize>, bool)> {
    let (start, end) = (selection.start(), selection.end());
    if selection.is_empty() || line < start.line || line > end.line {
        return None;
    }
    let from = if line == start.line { start.column } else { 0 };
    let to = if line == end.line {
        end.column
    } else {
        line_len
    };
    Some((from..to, line < end.line))
}

fn place_cursor(frame: &mut Frame, editor: &Editor, geometry: &Geometry) {
    let head = editor.selections().primary().head();
    let scroll = editor.scroll();
    let Ok(content) = editor.buffer().line_content(head.line) else {
        return;
    };
    let column = text_layout::display_column(&content, head.column);

    let (Some(row), Some(col)) = (
        head.line.checked_sub(scroll.top),
        column.checked_sub(scroll.left),
    ) else {
        return;
    };
    if row >= usize::from(geometry.text.height) || col >= usize::from(geometry.text.width) {
        return;
    }
    frame.set_cursor_position(ScreenPosition::new(
        geometry.text.x + col as u16,
        geometry.text.y + row as u16,
    ));
}
