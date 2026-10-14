//! Drawing one editor: a bordered frame with a line-number gutter and the
//! text. Works from a document and a view of it, so several panes can show
//! the same document.

use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Position as ScreenPosition, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::buffer::{Buffer, Position, Selection};
use crate::components::editor::text_layout::{self, digits, display_column, visible_cells};
use crate::components::editor::{EditorRef, ViewState, display_name};
use crate::components::find::Find;
use crate::lsp::{Diagnostic, Severity};
use crate::syntax::Span as SyntaxSpan;
use crate::ui::cursor::{dim_cell, extra_cell as extra_cursor_cell};
use crate::ui::pane_frame::PaneFrame;
use crate::ui::theme::Theme;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Smallest line-number column, in digits.
const MIN_GUTTER_DIGITS: usize = 3;

/// Records where the text will be drawn inside `area`, keeps the scroll inside
/// the text and, if the cursor, the text or the area changed, scrolls so the
/// cursor stays visible.
pub(crate) fn prepare(buffer: &Buffer, view: &mut ViewState, area: Rect, frame: &PaneFrame) {
    let geometry = Geometry::new(frame.inner(area), buffer.len_lines());
    view.set_viewport(geometry.text);
    view.set_gutter(geometry.gutter);
    view.clamp_scroll(buffer.len_lines());
    if view.take_view_change(buffer.version()) {
        keep_cursor_visible(buffer, view, &geometry);
    }
}

/// What one editor shows besides its frame.
pub(crate) struct Content<'a> {
    pub(crate) editor: EditorRef<'a>,
    /// The syntax colors of the text.
    pub(crate) syntax: &'a [SyntaxSpan],
    /// Only the focused editor gets the terminal cursor and the highlighted
    /// border.
    pub(crate) focused: bool,
    /// Whether typing goes to this editor: it gets the terminal cursor. Any
    /// other editor marks its cursor with a dimmed block.
    pub(crate) live: bool,
    pub(crate) find: Option<&'a Find>,
    /// What a language server says is wrong, in the order of the text.
    pub(crate) diagnostics: &'a [Diagnostic],
}

/// Draws the editor into `area` inside `pane_frame`.
pub(crate) fn draw(
    frame: &mut Frame,
    theme: &Theme,
    content: Content<'_>,
    area: Rect,
    pane_frame: &PaneFrame,
) {
    let Content {
        editor,
        syntax,
        focused,
        live,
        find,
        diagnostics,
    } = content;
    let (buffer, view) = (editor.buffer(), editor.view());
    let name = display_name(buffer);
    let block = pane_frame.block(theme, pane_frame.title_text(&name), focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let geometry = Geometry::new(inner, buffer.len_lines());
    let overlays = Overlays {
        syntax,
        find,
        diagnostics,
    };
    draw_lines(frame, theme, buffer, view, &overlays, &geometry);
    for selection in view.selections().iter() {
        if std::ptr::eq(selection, view.selections().primary()) {
            continue;
        }
        if let Some(cell) = cell_of(buffer, view, &geometry, selection.head()) {
            extra_cursor_cell(frame, theme, cell);
        }
    }
    match (cursor_cell(buffer, view, &geometry), live) {
        (Some(cell), true) => frame.set_cursor_position(cell),
        (Some(cell), false) => dim_cell(frame, theme, cell),
        (None, _) => {}
    }
}

fn keep_cursor_visible(buffer: &Buffer, view: &mut ViewState, geometry: &Geometry) {
    let head = view.selections().primary().head();
    let Ok(text) = buffer.line_content(head.line) else {
        return;
    };
    let column = display_column(&text, head.column, view.tab_width());
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

/// What is drawn over the text of an editor: colors, matches, diagnostics.
struct Overlays<'a> {
    syntax: &'a [SyntaxSpan],
    find: Option<&'a Find>,
    diagnostics: &'a [Diagnostic],
}

fn draw_lines(
    frame: &mut Frame,
    theme: &Theme,
    buffer: &Buffer,
    view: &ViewState,
    overlays: &Overlays<'_>,
    geometry: &Geometry,
) {
    let Overlays {
        syntax,
        find,
        diagnostics,
    } = *overlays;
    let scroll = view.scroll();
    let head_line = view.selections().primary().head().line;

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
        let line_start = buffer.line_start_byte(line);
        let next_start = buffer.line_start_byte(line + 1);
        let here: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|diagnostic| touches_line(diagnostic, line_start, next_start))
            .collect();
        numbers.push(Line::styled(
            format!("{:>width$} ", line + 1),
            theme.style(number_slot),
        ));

        let Ok(content) = buffer.line_content(line) else {
            continue;
        };
        let underlines = Underlines::new(theme, &here, line_start, &content);
        let cells = visible_cells(
            &content,
            scroll.left,
            usize::from(geometry.text.width),
            view.tab_width(),
        );
        let line_len = buffer.line_len(line).unwrap_or(0);
        let selected: Vec<(Range<usize>, bool)> = view
            .selections()
            .iter()
            .filter_map(|selection| selected_columns(selection, line, line_len))
            .collect();
        let colors = LineColors::new(syntax, buffer.line_start_byte(line), &content);

        let selection_style = theme.style("editor.selection");
        let match_style = theme.style("editor.match");
        let found = find.map_or(&[][..], |find| find.matches_on_line(line));
        let mut spans: Vec<Span> = cells
            .into_iter()
            .map(|cell| {
                let base = colors.style_of(cell.grapheme);
                let style = match &selected {
                    selected
                        if selected
                            .iter()
                            .any(|(range, _)| range.contains(&cell.grapheme)) =>
                    {
                        base.patch(selection_style)
                    }
                    _ if found
                        .iter()
                        .any(|m| (m.start..m.end).contains(&cell.grapheme)) =>
                    {
                        base.patch(match_style)
                    }
                    _ => base,
                };
                Span::styled(cell.text, underlines.apply(cell.grapheme, style))
            })
            .collect();
        let spacer = selected.iter().any(|(_, continues)| *continues);
        if spacer {
            spans.push(Span::styled(" ", selection_style));
        }
        // What the server says about this line, after the code, in the color of
        // its severity.
        if let Some((severity, message)) = inline_message(&here, line_start, next_start) {
            let line_width = display_column(&content, usize::MAX, view.tab_width());
            // Where the code ends in the window; a line that ends left of it has
            // scrolled away together with what is said about it.
            if let Some(end) = line_width.checked_sub(scroll.left) {
                let start = end + usize::from(spacer) + INLINE_GAP;
                let room = usize::from(geometry.text.width).saturating_sub(start);
                if room >= INLINE_MIN_WIDTH {
                    spans.push(Span::raw(" ".repeat(INLINE_GAP)));
                    spans.push(Span::styled(
                        fit(&message, room),
                        theme.style(slot_of(severity)),
                    ));
                }
            }
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

/// The screen cell of the primary cursor, when it is in view.
fn cursor_cell(buffer: &Buffer, view: &ViewState, geometry: &Geometry) -> Option<ScreenPosition> {
    cell_of(buffer, view, geometry, view.selections().primary().head())
}

/// The screen cell of `head`, when it is in view.
fn cell_of(
    buffer: &Buffer,
    view: &ViewState,
    geometry: &Geometry,
    head: Position,
) -> Option<ScreenPosition> {
    let scroll = view.scroll();
    let Ok(content) = buffer.line_content(head.line) else {
        return None;
    };
    let column = text_layout::display_column(&content, head.column, view.tab_width());

    let (Some(row), Some(col)) = (
        head.line.checked_sub(scroll.top),
        column.checked_sub(scroll.left),
    ) else {
        return None;
    };
    if row >= usize::from(geometry.text.height) || col >= usize::from(geometry.text.width) {
        return None;
    }
    Some(ScreenPosition::new(
        geometry.text.x + col as u16,
        geometry.text.y + row as u16,
    ))
}

/// The syntax colors of one line, by grapheme column.
struct LineColors<'a> {
    /// The spans that touch the line.
    spans: &'a [SyntaxSpan],
    /// Byte offset of the line in the text, and of each grapheme in the line.
    line_start: usize,
    grapheme_starts: Vec<usize>,
}

impl<'a> LineColors<'a> {
    fn new(spans: &'a [SyntaxSpan], line_start: usize, content: &str) -> Self {
        let line_end = line_start + content.len();
        let first = spans.partition_point(|span| span.range.end <= line_start);
        let touching = &spans[first..];
        let count = touching.partition_point(|span| span.range.start < line_end);
        let spans = &touching[..count];
        let grapheme_starts = if spans.is_empty() {
            Vec::new()
        } else {
            content.grapheme_indices(true).map(|(at, _)| at).collect()
        };
        Self {
            spans,
            line_start,
            grapheme_starts,
        }
    }

    fn style_of(&self, grapheme: usize) -> Style {
        let Some(offset) = self.grapheme_starts.get(grapheme) else {
            return Style::default();
        };
        let byte = self.line_start + offset;
        self.spans
            .iter()
            .find(|span| span.range.contains(&byte))
            .map_or_else(Style::default, |span| span.style)
    }
}

/// The theme slot of a severity.
pub(crate) fn slot_of(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "editor.diagnostic.error",
        Severity::Warning => "editor.diagnostic.warning",
        Severity::Information => "editor.diagnostic.info",
        Severity::Hint => "editor.diagnostic.hint",
    }
}

/// Whether `diagnostic` is about some of the line that starts at byte `start`
/// and is followed by the next line at `next`. A range ending where the line
/// starts is about the line before, and a point counts on the line it is on.
fn touches_line(diagnostic: &Diagnostic, start: usize, next: usize) -> bool {
    let range = &diagnostic.range;
    match range.is_empty() {
        true => range.start >= start && range.start < next.max(start + 1),
        false => range.start < next && range.end > start,
    }
}

/// The lines under the text of one line, by grapheme column.
struct Underlines {
    /// Grapheme ranges and the color of the line under each, the worst last so
    /// that it wins where two meet.
    ranges: Vec<(Range<usize>, Color)>,
}

impl Underlines {
    fn new(theme: &Theme, diagnostics: &[&Diagnostic], line_start: usize, content: &str) -> Self {
        if diagnostics.is_empty() {
            return Self { ranges: Vec::new() };
        }
        let starts: Vec<usize> = content.grapheme_indices(true).map(|(at, _)| at).collect();
        let mut ordered: Vec<&&Diagnostic> = diagnostics.iter().collect();
        // Worst last, so that it is drawn over a milder one.
        ordered.sort_by_key(|diagnostic| std::cmp::Reverse(diagnostic.severity));

        let ranges = ordered
            .into_iter()
            .filter_map(|diagnostic| {
                let style = theme.style(slot_of(diagnostic.severity));
                let color = style.underline_color.or(style.fg)?;
                let from = diagnostic.range.start.saturating_sub(line_start);
                let to = diagnostic.range.end.saturating_sub(line_start);
                let first = starts.partition_point(|at| *at < from);
                // A point is drawn under the character it is at.
                let last = match diagnostic.range.is_empty() {
                    true => first + 1,
                    false => starts.partition_point(|at| *at < to),
                };
                let end = last.min(starts.len());
                (first < end).then_some((first..end, color))
            })
            .collect();
        Self { ranges }
    }

    /// `style` with the line under it when a diagnostic is about the grapheme.
    fn apply(&self, grapheme: usize, style: Style) -> Style {
        match self
            .ranges
            .iter()
            .rev()
            .find(|(range, _)| range.contains(&grapheme))
        {
            Some((_, color)) => style
                .add_modifier(ratatui::style::Modifier::UNDERLINED)
                .underline_color(*color),
            None => style,
        }
    }
}

/// How many columns of space are kept between the end of the code and the
/// message of a diagnostic after it.
const INLINE_GAP: usize = 2;
/// The narrowest room worth putting a message in; with less it is left out.
const INLINE_MIN_WIDTH: usize = 8;

/// What is said after the code of a line about the diagnostics that start on
/// it: the first line of the worst one's message, with how many others there
/// are. A diagnostic over several lines is said once, on the line it starts on.
fn inline_message(
    diagnostics: &[&Diagnostic],
    line_start: usize,
    next_start: usize,
) -> Option<(Severity, String)> {
    let starting: Vec<&&Diagnostic> = diagnostics
        .iter()
        .filter(|diagnostic| {
            (line_start..next_start.max(line_start + 1)).contains(&diagnostic.range.start)
        })
        .collect();
    let worst = starting
        .iter()
        .min_by_key(|diagnostic| (diagnostic.severity, diagnostic.range.start))?;
    let first_line = worst.message.lines().next().unwrap_or_default().trim();
    let more = match starting.len() {
        1 => String::new(),
        others => format!(" (+{})", others - 1),
    };
    Some((worst.severity, format!("{first_line}{more}")))
}

/// `text` cut to `width` columns, with `…` where it was cut.
pub(crate) fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for character in text.chars() {
        let columns = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
        if used + columns + 1 > width {
            break;
        }
        out.push(character);
        used += columns;
    }
    out.push('…');
    out
}
