use std::sync::Arc;

use ratatui::layout::Rect;

use crate::buffer::{Position, Selections};
use crate::git::{Alignment, Side};

use super::{EditKind, IndentStyle, Scroll, TabWidth};

/// Everything about one editor view except the text itself: where the cursor
/// is, what is scrolled into view, and mouse and undo-grouping state. Several
/// views can show the same document.
#[derive(Debug, Default, Clone)]
pub(crate) struct ViewState {
    pub(super) selections: Selections,
    pub(super) scroll: Scroll,
    pub(super) viewport_height: usize,
    pub(super) text_area: Option<Rect>,
    /// Where the line numbers are, left of the text.
    pub(super) gutter: Option<Rect>,
    pub(super) drag_anchor: Option<Position>,
    /// The selections a drag that started with `alt` adds its selection to.
    pub(super) drag_base: Option<Selections>,
    /// The line a drag that started on a line number began on.
    pub(super) drag_lines: Option<usize>,
    pub(super) seen_view: Option<(Position, u64, Rect)>,
    pub(super) indent: IndentStyle,
    /// How wide a tab character draws.
    pub(super) tab_width: TabWidth,
    pub(super) last_edit: Option<EditKind>,
    /// Set when the rows are those of a diff, where a line may have filler rows around it.
    pub(super) aligned: Option<AlignedRows>,
}

/// How one side of a diff is laid out: a row of the screen is a row of the alignment, so
/// the rows of the two sides match.
#[derive(Debug, Clone)]
pub(crate) struct AlignedRows {
    alignment: Arc<Alignment>,
    side: Side,
    /// The row of the alignment at the top of the text.
    start: usize,
}

/// What stands in one row of the text area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Line(usize),
    /// Padding that keeps the other side of a diff in step: no line.
    Filler,
    /// Past the end of the text.
    End,
}

impl ViewState {
    pub(crate) fn with_indent(indent: IndentStyle) -> Self {
        Self {
            indent,
            ..Self::default()
        }
    }

    pub(crate) fn selections(&self) -> &Selections {
        &self.selections
    }

    pub(crate) fn scroll(&self) -> Scroll {
        self.scroll
    }

    /// Lines the text area shows, as of the last layout pass.
    pub(crate) fn viewport_height(&self) -> usize {
        self.viewport_height
    }

    pub(crate) fn text_area(&self) -> Option<Rect> {
        self.text_area
    }

    #[cfg(test)]
    pub(crate) fn indent(&self) -> IndentStyle {
        self.indent
    }

    pub(crate) fn set_indent(&mut self, indent: IndentStyle) {
        self.indent = indent;
    }

    pub(crate) fn tab_width(&self) -> TabWidth {
        self.tab_width
    }

    pub(crate) fn set_tab_width(&mut self, tab_width: TabWidth) {
        self.tab_width = tab_width;
    }

    /// Lays the view out in the rows of `alignment`, as `side` of it.
    pub(crate) fn set_aligned(&mut self, alignment: Arc<Alignment>, side: Side) {
        let start = alignment.row_of(side, self.scroll.top);
        self.aligned = Some(AlignedRows {
            alignment,
            side,
            start,
        });
    }

    /// Lays the view out in rows of its own again.
    pub(crate) fn clear_aligned(&mut self) {
        self.aligned = None;
    }

    /// The row of the alignment at the top of the text, when the view is part of a diff.
    pub(crate) fn aligned_start(&self) -> Option<usize> {
        self.aligned.as_ref().map(|aligned| aligned.start)
    }

    /// Starts the rows at `row`, for a view that follows another one.
    pub(crate) fn set_aligned_start(&mut self, row: usize) {
        if let Some(aligned) = &mut self.aligned {
            aligned.start = row;
        }
    }

    /// Takes the top of the rows from the scroll, after the scroll was set.
    pub(crate) fn sync_aligned_start(&mut self) {
        let top = self.scroll.top;
        if let Some(aligned) = &mut self.aligned {
            aligned.start = aligned.alignment.row_of(aligned.side, top);
        }
    }

    /// What is in the row `offset` rows below the top of the text, of a text with `lines`
    /// lines.
    pub(crate) fn row_at(&self, offset: usize, lines: usize) -> Row {
        match &self.aligned {
            None => {
                let line = self.scroll.top + offset;
                match line < lines {
                    true => Row::Line(line),
                    false => Row::End,
                }
            }
            Some(aligned) => {
                let row = aligned.start + offset;
                if row >= aligned.alignment.rows() {
                    return Row::End;
                }
                match aligned.alignment.line_at(aligned.side, row) {
                    Some(line) => Row::Line(line),
                    None => Row::Filler,
                }
            }
        }
    }

    /// Whether the row `offset` rows below the top is in a changed stretch of a diff.
    pub(crate) fn row_is_changed(&self, offset: usize) -> bool {
        self.aligned
            .as_ref()
            .is_some_and(|aligned| aligned.alignment.is_changed(aligned.start + offset))
    }

    /// Which side of a diff the view is, if it is part of one.
    pub(crate) fn diff_side(&self) -> Option<Side> {
        self.aligned.as_ref().map(|aligned| aligned.side)
    }

    /// How many rows below the top of the text `line` is, when it is not above the top.
    pub(crate) fn offset_of_line(&self, line: usize) -> Option<usize> {
        match &self.aligned {
            None => line.checked_sub(self.scroll.top),
            Some(aligned) => aligned
                .alignment
                .row_of(aligned.side, line)
                .checked_sub(aligned.start),
        }
    }

    /// The line a click `offset` rows below the top of the text means: the line in the row,
    /// and for a filler row the line above it. Without diff rows it is the line that many
    /// below the top, which may be past the end of the text.
    pub(crate) fn line_for_offset(&self, offset: usize) -> usize {
        match &self.aligned {
            None => self.scroll.top + offset,
            Some(aligned) => aligned
                .alignment
                .nearest_line(aligned.side, aligned.start + offset),
        }
    }

    /// A copy for a new pane showing the same document: same cursor and
    /// scroll, but nothing tied to the old pane's screen area.
    pub(crate) fn duplicate(&self) -> Self {
        Self {
            text_area: None,
            gutter: None,
            drag_anchor: None,
            drag_base: None,
            drag_lines: None,
            seen_view: None,
            last_edit: None,
            aligned: None,
            ..self.clone()
        }
    }

    /// Replaces the selections (used when another view edited the text) and
    /// forgets any pending typing burst. `version` is the buffer version the
    /// new selections belong to, so this view does not scroll to its cursor
    /// because of an edit it did not make.
    pub(crate) fn follow_remote_edit(&mut self, selections: Selections, version: u64) {
        self.selections = selections;
        self.last_edit = None;
        self.drag_anchor = None;
        self.drag_base = None;
        if let Some((_, _, area)) = self.seen_view {
            let head = self.selections.primary().head();
            self.seen_view = Some((head, version, area));
        }
    }

    /// Brings a view back that is scrolled past the end of a document of
    /// `line_count` lines. The text can get shorter under a view that is scrolled
    /// down (another view deleted it, or the file was reloaded); such a view
    /// would show nothing, so it shows the last page of the text instead. A
    /// view that is inside the text is left alone, so a wheel scroll that went
    /// as far as the last line stays.
    pub(crate) fn clamp_scroll(&mut self, line_count: usize) {
        if self.scroll.top >= line_count {
            self.scroll.top = line_count.saturating_sub(self.viewport_height);
        }
    }

    /// Forgets a pending typing burst, e.g. because another view edited the
    /// document in between.
    pub(crate) fn end_edit_run(&mut self) {
        self.last_edit = None;
    }

    /// Where the text is on screen, as last drawn.
    pub(crate) fn set_viewport(&mut self, text_area: Rect) {
        self.text_area = Some(text_area);
        self.viewport_height = usize::from(text_area.height);
    }

    /// The line whose marker (the last column of the gutter, where the marks of git, the
    /// lightbulb and the breakpoints are) is at a screen cell, when `lines` is how many
    /// lines the text has.
    pub(crate) fn marker_line_at(&self, column: u16, row: u16, lines: usize) -> Option<usize> {
        let (gutter, text) = (self.gutter?, self.text_area?);
        let marker = gutter.right().checked_sub(1)?;
        let on_marker =
            gutter.width >= 2 && column == marker && row >= text.y && row < text.bottom();
        let offset = usize::from(row.checked_sub(text.y)?);
        match self.row_at(offset, lines) {
            Row::Line(line) if on_marker => Some(line),
            _ => None,
        }
    }

    /// Where the line numbers are on screen, as last drawn.
    pub(crate) fn set_gutter(&mut self, gutter: Rect) {
        self.gutter = Some(gutter);
    }

    /// Whether the cursor, the buffer (by `version`) or the viewport changed
    /// since the last call. The view only scrolls to the cursor then, so that
    /// scrolling with the wheel is not undone by the next frame.
    pub(crate) fn take_view_change(&mut self, version: u64) -> bool {
        let Some(area) = self.text_area else {
            return true;
        };
        let now = (self.selections.primary().head(), version, area);
        let changed = self.seen_view != Some(now);
        self.seen_view = Some(now);
        changed
    }

    /// Scrolls the minimum needed to bring `line` and `display_column` into a
    /// viewport of `height` by `width` cells.
    pub(crate) fn scroll_to_show(
        &mut self,
        line: usize,
        display_column: usize,
        height: usize,
        width: usize,
    ) {
        self.viewport_height = height;
        self.scroll.top = match &self.aligned {
            None => super::scroll_axis(self.scroll.top, line, height),
            Some(aligned) => aligned_top(aligned, self.scroll.top, line, height),
        };
        self.scroll.left = super::scroll_axis(self.scroll.left, display_column, width);
    }
}

/// The top line that keeps `line` in view in a text of rows with filler in them: the rows,
/// not the lines, have to fit in `height`.
fn aligned_top(aligned: &AlignedRows, top: usize, line: usize, height: usize) -> usize {
    let alignment = &aligned.alignment;
    if height == 0 || line < top {
        return line.min(top);
    }
    let row = alignment.row_of(aligned.side, line);
    let top_row = alignment.row_of(aligned.side, top);
    if row < top_row + height {
        return top;
    }
    let wanted = row + 1 - height;
    let mut first = alignment.nearest_line(aligned.side, wanted);
    if alignment.row_of(aligned.side, first) < wanted {
        first += 1;
    }
    first.min(line)
}
