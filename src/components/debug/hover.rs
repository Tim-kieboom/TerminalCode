//! What the debugger says about an expression the pointer rests on (or the cursor is at), in a
//! small box next to it: the value, and for a struct or a list a tree that opens like the one of
//! the Debug pane. It comes from the session (`app/debug_hover.rs`), which asks the adapter and
//! fills the tree.
//!
//! A box from the pointer stays while the pointer is on it or its word; a click in it (or
//! `f6`) keeps it, and then it has the arrow keys until `esc` or a click elsewhere. When the
//! program stops again the session asks for the expression again and for the rows that were
//! open, so the box shows the program as it is.

use std::cell::Cell;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use super::model::{Incoming, Tree};
use super::view::variable_line;
use crate::components::editor::render::fit;
use crate::components::popups::diagnostic_popup::{place, wrap_text};
use crate::ids::DocumentId;
use crate::ui::theme::Theme;

/// The widest the box gets, and the narrowest when the screen allows.
const WIDEST: u16 = 90;
const NARROWEST: u16 = 24;
/// How many lines show at once; the rest scrolls.
const MOST_LINES: usize = 14;

/// What the box holds.
#[derive(Debug)]
enum Body {
    /// The expression came to something: the first row of the tree.
    Value,
    /// The adapter refused it: its words, as they are.
    Error(String),
}

#[derive(Debug)]
pub(crate) struct DebugHoverBox {
    expression: String,
    body: Body,
    tree: Tree,
    /// The cell the box hangs from.
    anchor: Position,
    /// What the expression was read from, so that an edit or another tab takes the box away.
    document: DocumentId,
    version: u64,
    /// Stays when the pointer moves, and has the keys.
    pinned: bool,
    selected: usize,
    scroll: usize,
    /// Paths of the rows to open again after the program stopped, as the answers come.
    restore: Vec<Vec<String>>,
    /// How many readings of children (for `restore`) are not answered yet.
    reading: usize,
    /// Where the box was drawn last, for the mouse.
    area: Cell<Rect>,
}

impl DebugHoverBox {
    /// A box for what `expression` came to.
    pub(crate) fn valued(
        expression: &str,
        root: Incoming,
        anchor: Position,
        document: DocumentId,
        version: u64,
        pinned: bool,
    ) -> Self {
        let mut tree = Tree::default();
        tree.set_root(expression, root);
        Self::with(
            expression,
            Body::Value,
            tree,
            anchor,
            document,
            version,
            pinned,
        )
    }

    /// A box for what the adapter said about an expression it would not evaluate.
    pub(crate) fn refused(
        expression: &str,
        message: String,
        anchor: Position,
        document: DocumentId,
        version: u64,
        pinned: bool,
    ) -> Self {
        Self::with(
            expression,
            Body::Error(message),
            Tree::default(),
            anchor,
            document,
            version,
            pinned,
        )
    }

    fn with(
        expression: &str,
        body: Body,
        tree: Tree,
        anchor: Position,
        document: DocumentId,
        version: u64,
        pinned: bool,
    ) -> Self {
        Self {
            expression: expression.to_owned(),
            body,
            tree,
            anchor,
            document,
            version,
            pinned,
            selected: 0,
            scroll: 0,
            restore: Vec::new(),
            reading: 0,
            area: Cell::new(Rect::default()),
        }
    }

    pub(crate) fn expression(&self) -> &str {
        &self.expression
    }

    pub(crate) fn document(&self) -> DocumentId {
        self.document
    }

    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    pub(crate) fn is_pinned(&self) -> bool {
        self.pinned
    }

    pub(crate) fn pin(&mut self) {
        self.pinned = true;
    }

    pub(crate) fn tree(&self) -> &Tree {
        &self.tree
    }

    pub(crate) fn tree_mut(&mut self) -> &mut Tree {
        &mut self.tree
    }

    /// The message of the adapter, when it refused the expression.
    #[cfg(test)]
    pub(crate) fn error(&self) -> Option<&str> {
        match &self.body {
            Body::Error(message) => Some(message),
            Body::Value => None,
        }
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        self.area.get().contains(Position::new(column, row))
    }

    // --- moving ---

    pub(crate) fn select(&mut self, row: usize) {
        self.selected = row.min(self.tree.rows().len().saturating_sub(1));
    }

    pub(crate) fn move_by(&mut self, rows: isize) {
        let last = self.tree.rows().len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(rows).min(last);
    }

    /// One page: what fits in the box, less a row.
    pub(crate) fn page(&self) -> isize {
        let height = usize::from(self.area.get().height.saturating_sub(2));
        isize::try_from(height.saturating_sub(1).max(1)).unwrap_or(1)
    }

    pub(crate) fn scroll_by(&mut self, rows: isize) {
        let most = self.tree.rows().len().saturating_sub(MOST_LINES);
        self.scroll = self.scroll.saturating_add_signed(rows).min(most);
    }

    /// The row of the tree under a screen cell.
    pub(crate) fn row_at(&self, column: u16, row: u16) -> Option<usize> {
        let area = self.area.get();
        let inside = area.contains(Position::new(column, row))
            && row > area.y
            && row + 1 < area.bottom()
            && matches!(self.body, Body::Value);
        let index = self.scroll + usize::from(row.saturating_sub(area.y + 1));
        (inside && index < self.tree.rows().len()).then_some(index)
    }

    // --- coming again after a stop ---

    /// The program stopped: what to ask again, and the paths of the rows to open again once
    /// the answer is there.
    pub(crate) fn begin_refresh(&mut self) -> Vec<Vec<String>> {
        self.tree.open_paths()
    }

    /// The expression came to `root` again: it replaces the first row, and `restore` are the rows
    /// to open again. What has to be read for them.
    pub(crate) fn refreshed(
        &mut self,
        root: Incoming,
        restore: Vec<Vec<String>>,
    ) -> Vec<super::model::Read> {
        self.body = Body::Value;
        self.tree.set_root(&self.expression.clone(), root);
        self.selected = 0;
        self.scroll = 0;
        self.restore = restore;
        self.reopen()
    }

    /// The adapter would not evaluate the expression again: its words take the place of the tree.
    pub(crate) fn refused_again(&mut self, message: String) {
        self.body = Body::Error(message);
        self.tree.clear();
        self.restore.clear();
        self.reading = 0;
    }

    /// Opens the rows that are still to be opened and have appeared; what to read for them.
    pub(crate) fn reopen(&mut self) -> Vec<super::model::Read> {
        let reads = self.tree.reopen(&mut self.restore);
        self.reading += reads.len();
        if self.reading == 0 {
            self.restore.clear();
        }
        reads
    }

    /// A reading of children is asked for (by a key or a click).
    pub(crate) fn read_started(&mut self) {
        self.reading += 1;
    }

    /// A reading of children was answered (or failed): the rows that were waiting for it
    /// are opened as far as they can be now. What to read next.
    pub(crate) fn read_done(&mut self) -> Vec<super::model::Read> {
        self.reading = self.reading.saturating_sub(1);
        self.reopen()
    }

    // --- drawing ---

    /// Draws the box under its anchor, above it when there is no room below.
    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        let lines = self.lines(usize::from(WIDEST.saturating_sub(4)), theme);
        let longest = lines.iter().map(Line::width).max().unwrap_or(0);
        let width = u16::try_from(longest + 4)
            .unwrap_or(u16::MAX)
            .clamp(NARROWEST, WIDEST)
            .min(screen.width);
        let text_width = usize::from(width.saturating_sub(4)).max(1);
        let lines = self.lines(text_width, theme);
        let rows = lines
            .len()
            .min(MOST_LINES)
            .min(usize::from(screen.height.saturating_sub(2)));
        let height = u16::try_from(rows).unwrap_or(u16::MAX) + 2;
        if rows == 0 || width < 5 {
            self.area.set(Rect::default());
            return;
        }
        let area = place(screen, self.anchor, width, height);
        self.area.set(area);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border"));
        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);
        let body = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };
        let first = self.scroll.min(lines.len().saturating_sub(rows));
        let selected_style = theme.style(match self.pinned {
            true => "errors.selected",
            false => "errors.selected.inactive",
        });
        let shown: Vec<Line> = lines
            .into_iter()
            .enumerate()
            .skip(first)
            .take(rows)
            .map(
                |(index, line)| match self.is_tree() && index == self.selected {
                    true => line.style(selected_style),
                    false => line,
                },
            )
            .collect();
        frame.render_widget(Paragraph::new(shown).style(theme.style("hover.text")), body);
    }

    fn is_tree(&self) -> bool {
        matches!(self.body, Body::Value)
    }

    /// The lines of the box for `width` columns: the rows of the tree, or the adapter's words.
    fn lines(&self, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        match &self.body {
            Body::Value => self
                .tree
                .rows()
                .iter()
                .map(|row| variable_line(theme, row, width))
                .collect(),
            Body::Error(message) => wrap_text(message, width)
                .into_iter()
                .map(|text| Line::from(Span::raw(fit(&text, width))))
                .collect(),
        }
    }
}
