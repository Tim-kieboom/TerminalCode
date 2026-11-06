use std::path::Path;

use tree_sitter::{Language as Grammar, WasmStore};

use crate::SyntaxError;
use crate::plugin::{self, PluginSlot};

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

/// JavaScript: the grammar's query is written for a highlighter where the last of two patterns
/// on the same text wins, and starts with `(identifier) @variable`, so here every function, call,
/// constructor, constant and built-in would be a variable. The patterns that are more specific
/// than that come first (the earlier one wins), and a tag of JSX is a keyword (themes have no `tag`).
const JAVASCRIPT_BEFORE: &str = r#"
(function_expression name: (identifier) @function)
(function_declaration name: (identifier) @function)
(method_definition name: (property_identifier) @function.method)
(pair key: (property_identifier) @function.method value: [(function_expression) (arrow_function)])
(assignment_expression left: (member_expression property: (property_identifier) @function.method) right: [(function_expression) (arrow_function)])
(variable_declarator name: (identifier) @function value: [(function_expression) (arrow_function)])
(assignment_expression left: (identifier) @function right: [(function_expression) (arrow_function)])
(call_expression function: (identifier) @function)
(call_expression function: (member_expression property: (property_identifier) @function.method))
((identifier) @function.builtin (#eq? @function.builtin "require") (#is-not? local))
((identifier) @variable.builtin (#match? @variable.builtin "^(arguments|module|console|window|document)$") (#is-not? local))
([(identifier) (shorthand_property_identifier) (shorthand_property_identifier_pattern)] @constant (#match? @constant "^[A-Z_][A-Z\\d_]+$"))
((identifier) @constructor (#match? @constructor "^[A-Z]"))
"#;

/// The tags of JSX, for the grammars that have them (JavaScript and TSX): a tag is a keyword
/// (themes have no `tag`), and a component is left to the constructor pattern above.
const JSX_BEFORE: &str = r#"
(jsx_opening_element (identifier) @keyword (#match? @keyword "^[a-z][^.]*$"))
(jsx_closing_element (identifier) @keyword (#match? @keyword "^[a-z][^.]*$"))
(jsx_self_closing_element (identifier) @keyword (#match? @keyword "^[a-z][^.]*$"))
"#;

/// TypeScript: what it adds to JavaScript, before the grammar's query for the same reason. The
/// names of types and of the things that hold them are types (the grammar's query only knows a
/// `type_identifier` where it is used), the members of an enum are constants, a decorator is an
/// attribute, and a parameter is a parameter (the grammar's pattern for it comes after the
/// JavaScript one that takes every identifier for a variable).
const TYPESCRIPT_BEFORE: &str = r#"
(interface_declaration name: (type_identifier) @type)
(type_alias_declaration name: (type_identifier) @type)
(enum_declaration name: (identifier) @type)
(class_declaration name: (type_identifier) @type)
(abstract_class_declaration name: (type_identifier) @type)
(type_parameter name: (type_identifier) @type)
(enum_body (property_identifier) @constant)
(enum_assignment name: (property_identifier) @constant)
(required_parameter pattern: (identifier) @variable.parameter)
(optional_parameter pattern: (identifier) @variable.parameter)
(decorator "@" @attribute)
(decorator (identifier) @attribute)
(decorator (call_expression function: (identifier) @attribute))
(decorator (member_expression property: (property_identifier) @attribute))
"#;

/// How a language writes a comment on one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comment {
    /// A marker at the start of the line: `//`, `#`.
    Line(&'static str),
    /// A pair around the line's text: `<!--` and `-->`.
    Around(&'static str, &'static str),
}

/// A language the editor can highlight; its grammar is compiled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Toml,
    Json,
    JavaScript,
    /// `.ts`, `.mts` and `.cts`.
    TypeScript,
    /// `.tsx`: TypeScript with JSX, which needs a grammar of its own because a cast written
    /// with angle brackets and a tag cannot be told apart.
    Tsx,
    Markdown,
    Nix,
    /// A language a plugin added (see [`crate::register_language`]).
    Plugin(PluginSlot),
}

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Toml => "TOML",
            Self::Json => "JSON",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Tsx => "TSX",
            Self::Markdown => "Markdown",
            Self::Nix => "Nix",
            Self::Plugin(slot) => plugin::get(slot).map_or("a language that is gone", |e| e.name),
        }
    }

    /// How the language writes a comment, for toggling one on a line. JSON takes `//`
    /// because the JSON files of the tools that read comments (and the language
    /// server's JSONC) do.
    pub fn comment(self) -> Option<Comment> {
        Some(match self {
            Self::Rust | Self::Json | Self::JavaScript | Self::TypeScript | Self::Tsx => {
                Comment::Line("//")
            }
            Self::Toml | Self::Nix => Comment::Line("#"),
            Self::Markdown => Comment::Around("<!--", "-->"),
            Self::Plugin(slot) => Comment::Line(plugin::get(slot)?.comment?),
        })
    }

    /// The language of the file at `path`, going by its extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        Self::from_extension(path.extension()?.to_str()?)
    }

    /// The language a name or an extension stands for (`"javascript"`, `"js"`), in any case: what
    /// an injection query names.
    pub fn by_name(name: &str) -> Option<Self> {
        let name = name.trim();
        [
            Self::Rust,
            Self::Toml,
            Self::Json,
            Self::JavaScript,
            Self::TypeScript,
            Self::Tsx,
            Self::Markdown,
            Self::Nix,
        ]
        .into_iter()
        .find(|language| language.name().eq_ignore_ascii_case(name))
        .or_else(|| plugin::by_name(name).map(Self::Plugin))
        .or_else(|| Self::from_extension(&name.to_ascii_lowercase()))
    }

    fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "rs" => Some(Self::Rust),
            "toml" => Some(Self::Toml),
            "json" | "jsonc" => Some(Self::Json),
            "js" | "mjs" | "cjs" | "jsx" => Some(Self::JavaScript),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "md" | "markdown" => Some(Self::Markdown),
            "nix" => Some(Self::Nix),
            other => plugin::by_extension(other).map(Self::Plugin),
        }
    }

    /// The query that says where other languages are inside this one, if it has one.
    pub(super) fn injections(self) -> Option<String> {
        match self {
            Self::Plugin(slot) => plugin::get(slot)
                .map(|e| e.injections.clone())
                .filter(|query| !query.trim().is_empty()),
            _ => None,
        }
    }

    /// The grammar, and for a language a plugin added the store it lives in, which the parser
    /// has to be given first.
    pub(super) fn grammar(self) -> Result<(Grammar, Option<WasmStore>), SyntaxError> {
        Ok((
            match self {
                Self::Rust => tree_sitter_rust::LANGUAGE.into(),
                Self::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
                Self::Json => tree_sitter_json::LANGUAGE.into(),
                Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
                Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
                Self::Markdown => tree_sitter_md::LANGUAGE.into(),
                Self::Nix => tree_sitter_nix::LANGUAGE.into(),
                Self::Plugin(slot) => {
                    let (grammar, store) = plugin::load(&*Self::plugin_entry(slot)?)?;
                    return Ok((grammar, Some(store)));
                }
            },
            None,
        ))
    }

    /// The tree-sitter query that names the highlightable parts of the tree: the
    /// grammar's own, with the editor's patterns before and after it.
    pub(super) fn highlights(self) -> String {
        let grammar = match self {
            Self::Plugin(slot) => {
                return plugin::get(slot).map_or_else(String::new, |e| e.highlights.clone());
            }
            Self::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY,
            Self::Toml => tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            Self::Json => tree_sitter_json::HIGHLIGHTS_QUERY,
            // The grammar parses JSX too, so its query for the tags comes with it.
            Self::JavaScript => &format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
            ),
            // TypeScript's own query only holds what it adds, and expects JavaScript's in front.
            Self::TypeScript => &format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            ),
            Self::Tsx => &format!(
                "{}\n{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            ),
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
    fn extra_highlights(self) -> (String, &'static str) {
        match self {
            Self::Rust => (RUST_BEFORE.to_owned(), RUST_AFTER),
            Self::JavaScript => (format!("{JAVASCRIPT_BEFORE}\n{JSX_BEFORE}"), ""),
            Self::TypeScript => (format!("{TYPESCRIPT_BEFORE}\n{JAVASCRIPT_BEFORE}"), ""),
            Self::Tsx => (
                format!("{TYPESCRIPT_BEFORE}\n{JAVASCRIPT_BEFORE}\n{JSX_BEFORE}"),
                "",
            ),
            _ => (String::new(), ""),
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
