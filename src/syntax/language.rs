use std::path::Path;

use tree_sitter::Language as Grammar;

/// Rust: control flow apart from the other keywords, numbers as numbers (the
/// grammar calls them constants), and a called method as a method: the grammar
/// has the same text as a property first, which wins the tie.
const RUST_BEFORE: &str = r#"
["break" "continue" "else" "for" "if" "in" "loop" "match" "return" "while" "yield"] @keyword.control
(integer_literal) @number
(float_literal) @number
(call_expression function: (field_expression field: (field_identifier) @function.method))
(generic_function function: (field_expression field: (field_identifier) @function.method))
"#;

/// Rust: the grammar leaves module names and plain variables uncolored. A path
/// segment is a namespace (the grammar already takes the capitalized ones for
/// types), and any identifier it has no capture for is a variable.
const RUST_AFTER: &str = r#"
(scoped_identifier path: (identifier) @namespace)
(scoped_identifier path: (scoped_identifier name: (identifier) @namespace))
(scoped_use_list path: (identifier) @namespace)
(scoped_use_list path: (scoped_identifier name: (identifier) @namespace))
(identifier) @variable
"#;

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

    /// The tree-sitter query that names the highlightable parts of the tree: the
    /// grammar's own, with the editor's patterns before and after it.
    pub(super) fn highlights(self) -> String {
        let grammar = match self {
            Self::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY,
            Self::Toml => tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            Self::Json => tree_sitter_json::HIGHLIGHTS_QUERY,
            Self::Markdown => tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            Self::Nix => tree_sitter_nix::HIGHLIGHTS_QUERY,
        };
        let (before, after) = self.extra_highlights();
        format!("{before}\n{grammar}\n{after}")
    }

    /// Patterns the editor adds to a grammar's query: `(before, after)`. Where
    /// two patterns capture exactly the same text the earlier one wins, so what
    /// comes `before` overrides the grammar (keywords it lumps together) and
    /// what comes `after` only fills in what the grammar leaves uncolored.
    fn extra_highlights(self) -> (&'static str, &'static str) {
        match self {
            Self::Rust => (RUST_BEFORE, RUST_AFTER),
            _ => ("", ""),
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
