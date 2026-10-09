//! Drawing one editor: a bordered frame with a line-number gutter and the
//! text. Works from a document and a view of it, so several panes can show
//! the same document.

use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Position as ScreenPosition, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::buffer::{Buffer, Selection};
use crate::components::editor::text_layout::{self, digits, display_column, visible_cells};
use crate::components::editor::{EditorRef, ViewState, display_name};
use crate::components::find::Find;
use crate::ui::pane_frame::PaneFrame;
use crate::ui::theme::Theme;

/// Smallest line-number column, in digits.
const MIN_GUTTER_DIGITS: usize = 3;

/// Records where the text will be drawn inside `area`, keeps the scroll inside
/// the text and, if the cursor, the text or the area changed, scrolls so the
/// cursor stays visible.
pub(crate) fn prepare(buffer: &Buffer, view: &mut ViewState, area: Rect, frame: &PaneFrame) {
    let geometry = Geometry::new(frame.inner(area), buffer.len_lines());
    view.set_viewport(geometry.text);
    view.clamp_scroll(buffer.len_lines());
    if view.take_view_change(buffer.version()) {
        keep_cursor_visible(buffer, view, &geometry);
    }
}

/// Draws the editor into `area` inside `pane_frame`. Only the focused editor
/// gets the terminal cursor and the highlighted border.
pub(crate) fn draw(
    frame: &mut Frame,
    theme: &Theme,
    editor: EditorRef<'_>,
    area: Rect,
    pane_frame: &PaneFrame,
    focused: bool,
    find: Option<&Find>,
) {
    let (buffer, view) = (editor.buffer(), editor.view());
    let name = display_name(buffer);
    let block = pane_frame.block(theme, pane_frame.title_text(&name), focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let geometry = Geometry::new(inner, buffer.len_lines());
    draw_lines(frame, theme, buffer, view, find, &geometry);
    if focused {
        place_cursor(frame, buffer, view, &geometry);
    }
}

fn keep_cursor_visible(buffer: &Buffer, view: &mut ViewState, geometry: &Geometry) {
    let head = view.selections().primary().head();
    let Ok(text) = buffer.line_content(head.line) else {
        return;
    };
    let column = display_column(&text, head.column);
    view.scroll_to_show(
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

fn draw_lines(
    frame: &mut Frame,
    theme: &Theme,
    buffer: &Buffer,
    view: &ViewState,
    find: Option<&Find>,
    geometry: &Geometry,
) {
    let scroll = view.scroll();
    let selection = view.selections().primary();
    let head_line = selection.head().line;

    let mut numbers = Vec::new();
    let mut texts = Vec::new();
    let visible = scroll.top..scroll.top + usize::from(geometry.text.height);
    for line in visible.take_while(|line| *line < buffer.len_lines()) {
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

        let Ok(content) = buffer.line_content(line) else {
            continue;
        };
        let cells = visible_cells(&content, scroll.left, usize::from(geometry.text.width));
        let selected = selected_columns(selection, line, buffer.line_len(line).unwrap_or(0));

        let selection_style = theme.style("editor.selection");
        let match_style = theme.style("editor.match");
        let found = find.map_or(&[][..], |find| find.matches_on_line(line));
        let mut spans: Vec<Span> = cells
            .into_iter()
            .map(|cell| {
                let style = match &selected {
                    Some((range, _)) if range.contains(&cell.grapheme) => selection_style,
                    _ if found
                        .iter()
                        .any(|m| (m.start..m.end).contains(&cell.grapheme)) =>
                    {
                        match_style
                    }
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

fn place_cursor(frame: &mut Frame, buffer: &Buffer, view: &ViewState, geometry: &Geometry) {
    let head = view.selections().primary().head();
    let scroll = view.scroll();
    let Ok(content) = buffer.line_content(head.line) else {
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
