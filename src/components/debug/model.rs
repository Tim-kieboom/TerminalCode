//! What the debugger shows: where the session is, the stack of the thread that stopped, and a
//! tree of variables of the chosen frame. It holds what the adapter said and nothing of how it
//! is drawn; the session (`app/debug.rs`) fills it and the pane draws it.
//!
//! The tree is a list of rows in the order they are drawn, each with its depth. A row with
//! children is opened to ask for them (one request for each, or one for each page of a long
//! list) and closed to forget them, so what is shown is never older than the stop.

use std::path::PathBuf;

/// How many children of a list are asked for at once; a longer list is read a page at a time.
pub(crate) const PAGE: i64 = 100;

/// Where the debug session is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum Status {
    /// There is no session.
    #[default]
    Idle,
    /// The adapter is started and the program is being set up (breakpoints, launch).
    Starting,
    /// The program is running.
    Running,
    Stopped {
        /// `breakpoint`, `step`, `exception`, `pause`, `entry`, ...
        reason: String,
        description: Option<String>,
    },
    /// The session is over; why, in words.
    Ended(String),
}

impl Status {
    /// Whether a session is going (the program may be running or stopped).
    pub(crate) fn is_active(&self) -> bool {
        matches!(self, Self::Starting | Self::Running | Self::Stopped { .. })
    }
}

/// One frame of the stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) id: i64,
    pub(crate) name: String,
    /// The file, when the frame has one (a frame in a library may not).
    pub(crate) path: Option<PathBuf>,
    /// The line, from 0.
    pub(crate) line: usize,
    pub(crate) column: usize,
}

/// What a row of the tree is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RowKind {
    /// A scope (`Locals`) or a variable.
    Variable,
    /// The rest of a long list, `loaded` of `total` being shown: opening it reads the next page.
    More {
        parent: u64,
        loaded: i64,
        total: i64,
    },
}

/// One row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VarRow {
    /// Names the row while the rows move (the replies of the adapter come later).
    pub(crate) id: u64,
    pub(crate) depth: usize,
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) type_name: Option<String>,
    /// Not 0 when it has children, which `variables` gets with this number.
    pub(crate) reference: i64,
    pub(crate) expanded: bool,
    pub(crate) loading: bool,
    pub(crate) kind: RowKind,
    /// How many children the adapter said it has (a list that is long is paged).
    total: Option<i64>,
}

impl VarRow {
    pub(crate) fn has_children(&self) -> bool {
        self.reference != 0 && self.kind == RowKind::Variable
    }
}

/// What the tree asks the session to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Read {
    pub(crate) row: u64,
    pub(crate) reference: i64,
    /// The first child wanted and how many, when it is a page of a long list.
    pub(crate) page: Option<(i64, i64)>,
}

/// A variable as the adapter said it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Incoming {
    pub(crate) name: String,
    pub(crate) value: String,
    pub(crate) type_name: Option<String>,
    pub(crate) reference: i64,
    /// How many children it has, when the adapter says.
    pub(crate) total: Option<i64>,
}

/// The tree of variables of a frame.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Tree {
    rows: Vec<VarRow>,
    next_id: u64,
}

impl Tree {
    pub(crate) fn rows(&self) -> &[VarRow] {
        &self.rows
    }

    pub(crate) fn clear(&mut self) {
        self.rows.clear();
    }

    fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn position(&self, id: u64) -> Option<usize> {
        self.rows.iter().position(|row| row.id == id)
    }

    /// Starts again with the scopes of a frame, each a row at the top that opens to its variables.
    pub(crate) fn set_scopes(&mut self, scopes: &[(String, i64)]) {
        self.rows.clear();
        for (name, reference) in scopes {
            let id = self.new_id();
            self.rows.push(VarRow {
                id,
                depth: 0,
                name: name.clone(),
                value: String::new(),
                type_name: None,
                reference: *reference,
                expanded: false,
                loading: false,
                kind: RowKind::Variable,
                total: None,
            });
        }
    }

    /// Opens (or, if it is open, closes) the row at `index`. What has to be read to open it.
    pub(crate) fn toggle(&mut self, index: usize) -> Option<Read> {
        let row = self.rows.get(index)?;
        if let RowKind::More {
            parent,
            loaded,
            total,
        } = row.kind.clone()
        {
            return self.read_more(parent, loaded, total);
        }
        if row.expanded {
            self.collapse(index);
            return None;
        }
        if !row.has_children() {
            return None;
        }
        let id = row.id;
        let reference = row.reference;
        let page = row.total.filter(|total| *total > PAGE).map(|_| (0, PAGE));
        let row = &mut self.rows[index];
        row.expanded = true;
        row.loading = true;
        Some(Read {
            row: id,
            reference,
            page,
        })
    }

    fn read_more(&mut self, parent: u64, loaded: i64, total: i64) -> Option<Read> {
        let at = self.position(parent)?;
        let row = &mut self.rows[at];
        row.loading = true;
        let count = PAGE.min(total - loaded).max(1);
        Some(Read {
            row: parent,
            reference: row.reference,
            page: Some((loaded, count)),
        })
    }

    /// Forgets the children of the row at `index`.
    pub(crate) fn collapse(&mut self, index: usize) {
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };
        row.expanded = false;
        row.loading = false;
        let depth = row.depth;
        let end = self.rows[index + 1..]
            .iter()
            .position(|below| below.depth <= depth)
            .map_or(self.rows.len(), |offset| index + 1 + offset);
        self.rows.drain(index + 1..end);
    }

    /// The children of the row `row` arrived: `children` are the ones from `start` on. A page
    /// after the first is added to what is shown; the first replaces it. A row that is gone, or
    /// was closed meanwhile, takes nothing.
    pub(crate) fn fill(&mut self, row: u64, start: i64, children: Vec<Incoming>) {
        let Some(at) = self.position(row) else {
            return;
        };
        if !self.rows[at].expanded {
            return;
        }
        self.rows[at].loading = false;
        let depth = self.rows[at].depth + 1;
        let total = self.rows[at].total;
        if start == 0 {
            // The first page replaces what was shown.
            self.collapse_children(at);
        } else {
            self.remove_more(at);
        }
        let shown = self.count_children(at);
        let mut inserted: Vec<VarRow> = Vec::with_capacity(children.len() + 1);
        for child in &children {
            let id = self.new_id();
            inserted.push(VarRow {
                id,
                depth,
                name: child.name.clone(),
                value: child.value.clone(),
                type_name: child.type_name.clone(),
                reference: child.reference,
                expanded: false,
                loading: false,
                kind: RowKind::Variable,
                total: child.total,
            });
        }
        let loaded = shown + i64::try_from(children.len()).unwrap_or(0);
        if let Some(total) = total
            && loaded < total
            && !children.is_empty()
        {
            let id = self.new_id();
            inserted.push(VarRow {
                id,
                depth,
                name: String::new(),
                value: format!("… {} more", total - loaded),
                type_name: None,
                reference: 0,
                expanded: false,
                loading: false,
                kind: RowKind::More {
                    parent: row,
                    loaded,
                    total,
                },
                total: None,
            });
        }
        let at_end = self.end_of_children(at);
        self.rows.splice(at_end..at_end, inserted);
    }

    /// The row asked for children and the adapter refused or went away: it is closed again.
    pub(crate) fn failed(&mut self, row: u64) {
        if let Some(at) = self.position(row) {
            self.collapse(at);
        }
    }

    fn collapse_children(&mut self, at: usize) {
        let depth = self.rows[at].depth;
        let end = self.rows[at + 1..]
            .iter()
            .position(|below| below.depth <= depth)
            .map_or(self.rows.len(), |offset| at + 1 + offset);
        self.rows.drain(at + 1..end);
    }

    fn end_of_children(&self, at: usize) -> usize {
        let depth = self.rows[at].depth;
        self.rows[at + 1..]
            .iter()
            .position(|below| below.depth <= depth)
            .map_or(self.rows.len(), |offset| at + 1 + offset)
    }

    /// How many children (rows that are variables) the row at `at` shows now.
    fn count_children(&self, at: usize) -> i64 {
        let depth = self.rows[at].depth + 1;
        let end = self.end_of_children(at);
        let shown = self.rows[at + 1..end]
            .iter()
            .filter(|row| row.depth == depth && row.kind == RowKind::Variable)
            .count();
        i64::try_from(shown).unwrap_or(0)
    }

    fn remove_more(&mut self, at: usize) {
        let end = self.end_of_children(at);
        let depth = self.rows[at].depth + 1;
        if let Some(offset) = self.rows[at + 1..end]
            .iter()
            .position(|row| row.depth == depth && matches!(row.kind, RowKind::More { .. }))
        {
            self.rows.remove(at + 1 + offset);
        }
    }
}

/// Everything the Debug pane shows.
#[derive(Debug, Default)]
pub(crate) struct DebugData {
    pub(crate) status: Status,
    pub(crate) frames: Vec<Frame>,
    pub(crate) selected_frame: usize,
    pub(crate) tree: Tree,
}

impl DebugData {
    /// The program runs again, or the session is over: nothing of the last stop is true.
    pub(crate) fn forget_stop(&mut self) {
        self.frames.clear();
        self.selected_frame = 0;
        self.tree.clear();
    }

    /// The frame chosen, the one the editor shows.
    pub(crate) fn frame(&self) -> Option<&Frame> {
        self.frames.get(self.selected_frame)
    }
}
