use ratatui::layout::Rect;

use crate::buffer::{Position, Selections};

use super::{EditKind, IndentStyle, Scroll};

/// Everything about one editor view except the text itself: where the cursor
/// is, what is scrolled into view, and mouse and undo-grouping state. Several
/// views can show the same document.
#[derive(Debug, Default, Clone)]
pub(crate) struct ViewState {
    pub(super) selections: Selections,
    pub(super) scroll: Scroll,
    pub(super) viewport_height: usize,
    pub(super) text_area: Option<Rect>,
    pub(super) drag_anchor: Option<Position>,
    pub(super) seen_view: Option<(Position, u64, Rect)>,
    pub(super) indent: IndentStyle,
    pub(super) last_edit: Option<EditKind>,
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

    #[cfg(test)]
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

    /// A copy for a new pane showing the same document: same cursor and
    /// scroll, but nothing tied to the old pane's screen area.
    pub(crate) fn duplicate(&self) -> Self {
        Self {
            text_area: None,
            drag_anchor: None,
            seen_view: None,
            last_edit: None,
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
        self.scroll.top = super::scroll_axis(self.scroll.top, line, height);
        self.scroll.left = super::scroll_axis(self.scroll.left, display_column, width);
    }
}
