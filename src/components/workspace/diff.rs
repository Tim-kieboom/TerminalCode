//! A tab that shows a file next to the file as it is in `HEAD`, the changes lined up row
//! for row (see [`Alignment`]). The right side is the document itself, the same one a plain
//! tab of the file shows, so it is edited and saved as usual; the left side is a locked copy
//! of the text in `HEAD`, which the tab holds itself.

use std::path::PathBuf;
use std::sync::Arc;

use ratatui::layout::Rect;

use super::{Tab, Workspace};
use crate::buffer::{Buffer, Position};
use crate::components::editor::ViewState;
use crate::git::{Alignment, Side, hash_lines, line_hashes};
use crate::ids::DocumentId;
use crate::syntax::DocumentSyntax;
use crate::syntax::SyntaxWorker;

pub(crate) use crate::git::{MAX_DIFF_BYTES as MAX_BYTES, MAX_DIFF_LINES as MAX_LINES};

/// The left side of a diff tab, and how the two sides line up.
#[derive(Debug)]
pub(super) struct DiffTab {
    /// The text of the file in `HEAD`; locked.
    base: Buffer,
    view: ViewState,
    syntax: DocumentSyntax,
    hashes: Vec<u64>,
    /// Counts the texts taken as the base, so a new one is noticed.
    base_number: u64,
    alignment: Arc<Alignment>,
    /// The version of the document and the base the alignment is for.
    worked_out_for: Option<(u64, u64)>,
    /// The commit the base is from.
    head: Option<String>,
    /// The path of the file from the root of the repository.
    relative: PathBuf,
    /// The name of the file with `(HEAD)`, which is what the left frame says.
    title: String,
    /// The document grew past the limit after the tab was made: no comparison is made.
    too_large: bool,
}

/// What is known of a diff tab from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiffInfo {
    pub(crate) document: DocumentId,
    pub(crate) relative: PathBuf,
    pub(crate) head: Option<String>,
}

/// Why a file cannot be shown in a diff.
pub(crate) fn too_large(name: &str) -> String {
    format!(
        "{name} is too large to compare (more than {MAX_LINES} lines or {} MB)",
        MAX_BYTES / (1024 * 1024)
    )
}

/// Whether a text is too big to compare.
fn is_too_large(lines: usize, bytes: usize) -> bool {
    lines > MAX_LINES || bytes > MAX_BYTES
}

impl DiffTab {
    fn new(key: u64, relative: PathBuf, name: &str, head: Option<String>, base: &str) -> Self {
        let mut tab = Self {
            base: Buffer::default(),
            view: ViewState::default(),
            syntax: DocumentSyntax::new(key).with_language_of(&relative),
            hashes: Vec::new(),
            base_number: 0,
            alignment: Arc::new(Alignment::default()),
            worked_out_for: None,
            head: None,
            relative,
            title: format!("{name} (HEAD)"),
            too_large: false,
        };
        tab.set_base(head, base);
        tab
    }

    /// Takes `text` as the left side.
    fn set_base(&mut self, head: Option<String>, text: &str) {
        self.base = Buffer::defaults_copy(text, self.title.clone());
        self.hashes = hash_lines(text);
        self.base_number += 1;
        self.head = head;
        self.syntax.start_over();
    }

    pub(super) fn key(&self) -> u64 {
        self.syntax.key()
    }

    /// What the row above the left side says: where the text is from, and when there is
    /// nothing to see.
    fn heading(&self) -> String {
        let commit = match &self.head {
            Some(oid) => format!("HEAD {}", oid.get(..7).unwrap_or(oid)),
            None => "HEAD".to_owned(),
        };
        match (self.too_large, self.alignment.is_same()) {
            (true, _) => format!(" {commit} - too large to compare"),
            (false, true) => format!(" {commit} - no changes"),
            (false, false) => format!(" {commit}"),
        }
    }

    /// Works the alignment out again when the document or the base changed since, and gives
    /// both views the rows of it. Past the limit the sides are drawn on their own.
    fn refresh(&mut self, document: &Buffer, view: &mut ViewState) {
        let key = (document.version(), self.base_number);
        if self.worked_out_for == Some(key) {
            return;
        }
        self.worked_out_for = Some(key);
        self.too_large = is_too_large(document.len_lines(), document.len_bytes());
        if self.too_large {
            self.alignment = Arc::new(Alignment::default());
            view.clear_aligned();
            self.view.clear_aligned();
            return;
        }
        self.alignment = Arc::new(Alignment::new(&self.hashes, &line_hashes(document)));
        view.set_aligned(Arc::clone(&self.alignment), Side::New);
        self.view
            .set_aligned(Arc::clone(&self.alignment), Side::Old);
    }

    /// Asks the syntax worker for the colors of the part of the left text in view.
    fn update_syntax(&mut self, worker: &SyntaxWorker) -> Option<String> {
        let height = self.view.viewport_height();
        let top = self.view.scroll().top;
        let first = self.base.line_start_byte(top.saturating_sub(height));
        let last = self.base.line_start_byte(top + 2 * height);
        self.syntax.update(&self.base, &[first..last], worker);
        self.syntax
            .take_error()
            .map(|error| format!("{}: {error}", self.title))
    }
}

/// The row above the two sides, which says what each is, and the room that is left below it.
pub(super) fn header_and_body(area: Rect) -> (Rect, Rect) {
    let header = Rect {
        height: area.height.min(1),
        ..area
    };
    let body = Rect {
        y: area.y + header.height,
        height: area.height - header.height,
        ..area
    };
    (header, body)
}

/// Where the two sides of a diff go inside an area: the old text on the left.
pub(super) fn sides_of(area: Rect) -> (Rect, Rect) {
    let left = Rect {
        width: area.width / 2,
        ..area
    };
    let right = Rect {
        x: area.x + left.width,
        width: area.width - left.width,
        ..area
    };
    (left, right)
}

impl Workspace {
    /// Shows `document` next to `base`, its text in the commit `head` (`None` for a file that
    /// is not in git yet: nothing on the left). A tab that shows this document's diff is
    /// brought to the front and gets the new base; otherwise a tab is made in the focused
    /// pane. `relative` is the path of the file from the root of the repository. The reason
    /// when it cannot be shown.
    pub(crate) fn open_diff(
        &mut self,
        document: DocumentId,
        relative: PathBuf,
        head: Option<String>,
        base: Option<&str>,
    ) -> Result<(), String> {
        let name = crate::components::editor::display_name(&self.documents[&document].buffer);
        self.check_sizes(document, base, &name)?;
        let base = base.unwrap_or_default();
        if let Some((pane, tab)) = self.find_diff_tab(document) {
            self.reset_confirmations();
            self.focused = self.panes[pane].id;
            self.panes[pane].active = tab;
            if let Some(diff) = self.panes[pane].tabs[tab].diff.as_deref_mut() {
                diff.set_base(head, base);
            }
            return Ok(());
        }

        self.reset_confirmations();
        // Start where a plain tab of the file has its cursor and scroll.
        let view = self
            .panes
            .iter()
            .flat_map(|pane| &pane.tabs)
            .find(|tab| tab.document == document && tab.diff.is_none())
            .map(|tab| tab.view.duplicate())
            .unwrap_or_default();
        let key = u64::from(self.take_id());
        let mut tab = self.new_tab(document, view);
        tab.diff = Some(Box::new(DiffTab::new(key, relative, &name, head, base)));
        let pane = self.focused_pane_mut();
        pane.tabs.push(tab);
        pane.active = pane.tabs.len() - 1;
        Ok(())
    }

    /// Shows a file that is gone from the disk against its text in `HEAD`: the right side is
    /// empty and cannot be edited. `relative` is the path of the file from the root of the
    /// repository.
    pub(crate) fn open_deleted_diff(
        &mut self,
        relative: PathBuf,
        head: Option<String>,
        base: &str,
    ) -> Result<(), String> {
        let name = relative.display().to_string();
        let lines = base.bytes().filter(|byte| *byte == b'\n').count() + 1;
        if is_too_large(lines, base.len()) {
            return Err(too_large(&name));
        }
        let buffer = Buffer::defaults_copy("", format!("{name} (deleted)"));
        let document = self.add_document(buffer);
        self.open_diff(document, relative, head, Some(base))
    }

    /// Gives every diff tab of `document` a new base. The reason when the text is too big,
    /// and then the tabs are left as they were.
    pub(crate) fn set_diff_base(
        &mut self,
        document: DocumentId,
        head: Option<String>,
        base: Option<&str>,
    ) -> Result<(), String> {
        let name = crate::components::editor::display_name(&self.documents[&document].buffer);
        self.check_sizes(document, base, &name)?;
        let base = base.unwrap_or_default();
        for tab in self.panes.iter_mut().flat_map(|pane| &mut pane.tabs) {
            if tab.document != document {
                continue;
            }
            if let Some(diff) = tab.diff.as_deref_mut() {
                diff.set_base(head.clone(), base);
            }
        }
        Ok(())
    }

    fn check_sizes(
        &self,
        document: DocumentId,
        base: Option<&str>,
        name: &str,
    ) -> Result<(), String> {
        let buffer = &self.documents[&document].buffer;
        let base = base.unwrap_or_default();
        let base_lines = base.bytes().filter(|byte| *byte == b'\n').count() + 1;
        match is_too_large(buffer.len_lines(), buffer.len_bytes())
            || is_too_large(base_lines, base.len())
        {
            true => Err(too_large(name)),
            false => Ok(()),
        }
    }

    fn find_diff_tab(&self, document: DocumentId) -> Option<(usize, usize)> {
        self.panes.iter().enumerate().find_map(|(pane, p)| {
            let tab = p
                .tabs
                .iter()
                .position(|tab| tab.document == document && tab.diff.is_some())?;
            Some((pane, tab))
        })
    }

    /// The diff tabs there are, for the commit their base is from to be looked at again.
    pub(crate) fn diffs(&self) -> Vec<DiffInfo> {
        self.panes
            .iter()
            .flat_map(|pane| &pane.tabs)
            .filter_map(|tab| {
                let diff = tab.diff.as_deref()?;
                Some(DiffInfo {
                    document: tab.document,
                    relative: diff.relative.clone(),
                    head: diff.head.clone(),
                })
            })
            .collect()
    }

    /// Whether the tab in front of the focused pane is a diff.
    #[cfg(test)]
    pub(crate) fn active_is_diff(&self) -> bool {
        self.active_tab().is_some_and(|tab| tab.diff.is_some())
    }

    /// Moves the cursor of the diff in front to the next change (or the one before). `None`
    /// when the tab in front is no diff; else whether there was one to go to.
    pub(crate) fn diff_jump(&mut self, forward: bool) -> Option<bool> {
        let pane = self.focused_index();
        let active = self.panes[pane].active;
        let tab = self.panes[pane].tabs.get_mut(active)?;
        let diff = tab.diff.as_deref_mut()?;
        diff.refresh(&self.documents[&tab.document].buffer, &mut tab.view);
        let line = tab.view.selections().primary().head().line;
        let target = match forward {
            true => diff.alignment.next_change(line),
            false => diff.alignment.previous_change(line),
        };
        let Some(target) = target else {
            return Some(false);
        };
        self.with_editor(|editor| editor.go_to(Position::new(target, 0)));
        Some(true)
    }

    /// Works out the alignment of the tab in front of pane `index` and the places of the two
    /// sides, and tells where the sides go.
    pub(super) fn layout_diff(
        &mut self,
        index: usize,
        frame: &crate::ui::pane_frame::PaneFrame,
    ) -> bool {
        let pane = &mut self.panes[index];
        let area = pane.editor_area;
        let Some(tab) = pane.tabs.get_mut(pane.active) else {
            return false;
        };
        let Some(diff) = tab.diff.as_deref_mut() else {
            return false;
        };
        let document = &self.documents[&tab.document].buffer;
        diff.refresh(document, &mut tab.view);
        let (_, body) = header_and_body(area);
        let (left, right) = sides_of(body);
        crate::components::editor::render::prepare(document, &mut tab.view, right, frame);
        let start = tab.view.aligned_start().unwrap_or(0);
        crate::components::editor::render::prepare_following(
            &diff.base,
            &mut diff.view,
            left,
            frame,
            start,
        );
        true
    }

    /// Asks the syntax worker for the colors of the old side of the diffs on screen.
    pub(super) fn refresh_diff_highlights(&mut self, worker: &SyntaxWorker) -> Vec<String> {
        let mut errors = Vec::new();
        for pane in &mut self.panes {
            let Some(tab) = pane.tabs.get_mut(pane.active) else {
                continue;
            };
            if let Some(diff) = tab.diff.as_deref_mut() {
                errors.extend(diff.update_syntax(worker));
            }
        }
        errors
    }

    /// Hands the colors the worker made to the old side of a diff, when they are for it.
    pub(super) fn accept_diff_highlights(&mut self, output: crate::syntax::Output) -> Option<bool> {
        let diff = self
            .panes
            .iter_mut()
            .flat_map(|pane| &mut pane.tabs)
            .filter_map(|tab| tab.diff.as_deref_mut())
            .find(|diff| diff.syntax.key() == output.key)?;
        Some(diff.syntax.accept(output))
    }

    /// Has every old side ask the worker for its colors again.
    pub(super) fn restart_diff_highlights(&mut self) {
        for tab in self.panes.iter_mut().flat_map(|pane| &mut pane.tabs) {
            if let Some(diff) = tab.diff.as_deref_mut() {
                diff.syntax.start_over();
            }
        }
    }
}

/// What the left side of a diff tab draws.
pub(super) struct DiffSide<'a> {
    pub(super) buffer: &'a Buffer,
    pub(super) view: &'a ViewState,
    pub(super) syntax: &'a DocumentSyntax,
    /// What the frame of the side says.
    pub(super) title: &'a str,
    /// What the row above the side says.
    pub(super) heading: String,
}

impl Tab {
    /// What a diff of this tab draws on its left side, and the title of it.
    pub(super) fn diff_side(&self) -> Option<DiffSide<'_>> {
        let diff = self.diff.as_deref()?;
        Some(DiffSide {
            buffer: &diff.base,
            view: &diff.view,
            syntax: &diff.syntax,
            title: &diff.title,
            heading: diff.heading(),
        })
    }
}
