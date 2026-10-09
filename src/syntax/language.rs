use std::path::Path;

use tree_sitter::Language as Grammar;

/// A language the editor can highlight; its grammar is compiled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Language {
    Rust,
    Toml,
    Json,
    Markdown,
    Nix,
}

impl Language {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Toml => "TOML",
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
            Self::Nix => "Nix",
        }
    }

    /// The language of the file at `path`, going by its extension.
    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "rs" => Some(Self::Rust),
            "toml" => Some(Self::Toml),
            "json" => Some(Self::Json),
            "md" | "markdown" => Some(Self::Markdown),
            "nix" => Some(Self::Nix),
            _ => None,
        }
    }

    pub(super) fn grammar(self) -> Grammar {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Self::Json => tree_sitter_json::LANGUAGE.into(),
            Self::Markdown => tree_sitter_md::LANGUAGE.into(),
            Self::Nix => tree_sitter_nix::LANGUAGE.into(),
        }
    }

    /// The tree-sitter query that names the highlightable parts of the tree.
    pub(super) fn highlights(self) -> &'static str {
        match self {
            Self::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY,
            Self::Toml => tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            Self::Json => tree_sitter_json::HIGHLIGHTS_QUERY,
            Self::Markdown => tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            Self::Nix => tree_sitter_nix::HIGHLIGHTS_QUERY,
        }
    }

    /// For a language whose text holds another language's (Markdown's
    /// paragraphs are parsed with a second grammar): the inner grammar, its
    /// highlight query, and the node kind of the block tree it is parsed for.
    pub(super) fn inner(self) -> Option<Inner> {
        match self {
            Self::Markdown => Some(Inner {
                grammar: tree_sitter_md::INLINE_LANGUAGE.into(),
                highlights: tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
                node_kind: "inline",
            }),
            _ => None,
        }
    }
}

/// The language inside a language, see [`Language::inner`].
pub(super) struct Inner {
    pub(super) grammar: Grammar,
    pub(super) highlights: &'static str,
    pub(super) node_kind: &'static str,
}
