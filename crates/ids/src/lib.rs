//! The ids that name documents, views and panes. They live here, not with the
//! workspace that hands them out, so that code that only needs to refer to a
//! document (the language servers, the jump list, the diagnostics) does not depend on
//! the editor's UI.

/// A file open in the editor, stored once however many tabs show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(u32);

/// A tab: one view of a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewId(u32);

/// A strip of tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(u32);

macro_rules! numbered {
    ($($id:ident),*) => {$(
        impl $id {
            /// The id with this number; whoever hands ids out keeps them apart.
            pub const fn new(number: u32) -> Self {
                Self(number)
            }

        }
    )*};
}

numbered!(DocumentId, ViewId, PaneId);

impl DocumentId {
    /// The number of the id.
    pub const fn number(self) -> u32 {
        self.0
    }
}
