use std::path::Path;

use tree_sitter::Language as Grammar;

/// A language the editor can highlight; its grammar is compiled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Language {
    Rust,
}

impl Language {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
        }
    }

    /// The language of the file at `path`, going by its extension.
    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "rs" => Some(Self::Rust),
            _ => None,
        }
    }

    pub(super) fn grammar(self) -> Grammar {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }

    /// The tree-sitter query that names the highlightable parts of the tree.
    pub(super) fn highlights(self) -> &'static str {
        match self {
            Self::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY,
        }
    }
}
