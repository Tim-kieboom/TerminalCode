//! Telling a server about the documents it serves: `didOpen` when it first
//! sees one, `didChange` with what changed since it last looked, `didSave` after
//! a save and `didClose` when the document goes away. It decides what to send;
//! the app sends it.
//!
//! The state is a copy of each document as the server has it (a rope clone,
//! which shares its text), so a change is worked out against what the server
//! really knows, whatever happened to the text in between.

use std::collections::HashMap;

use ropey::Rope;
use serde_json::{Value, json};

use super::config::ServerKey;
use super::diff::change_between;
use super::position::LspPosition;
use crate::components::workspace::DocumentId;

/// A document as it is now.
#[derive(Debug, Clone)]
pub(crate) struct DocSnapshot {
    pub(crate) id: DocumentId,
    pub(crate) uri: String,
    pub(crate) language_id: String,
    pub(crate) text: Rope,
    /// Grows with every change to the text.
    pub(crate) version: u64,
    /// Grows with every save.
    pub(crate) saves: u64,
}

/// A message for the server.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Outgoing {
    pub(crate) method: &'static str,
    pub(crate) params: Value,
}

/// What a server has been told about a document.
#[derive(Debug)]
struct Told {
    uri: String,
    text: Rope,
    version: u64,
    saves: u64,
}

#[derive(Debug, Default)]
struct ServerDocs {
    /// Which run of the server this is; a new one knows nothing.
    generation: u64,
    open: HashMap<DocumentId, Told>,
}

#[derive(Debug, Default)]
pub(crate) struct DocumentSync {
    servers: HashMap<ServerKey, ServerDocs>,
}

impl DocumentSync {
    /// What to tell the server `key`, in its run number `generation`, so that it
    /// has exactly `docs` open and as they are now.
    pub(crate) fn messages(
        &mut self,
        key: &ServerKey,
        generation: u64,
        docs: &[DocSnapshot],
    ) -> Vec<Outgoing> {
        let server = self.servers.entry(key.clone()).or_default();
        if server.generation != generation {
            // A server that started again has nothing open.
            *server = ServerDocs {
                generation,
                open: HashMap::new(),
            };
        }

        let mut closes = Vec::new();
        let mut messages = Vec::new();
        // Documents that went away, or that now have another name.
        server.open.retain(|id, told| {
            let kept = docs.iter().any(|doc| doc.id == *id && doc.uri == told.uri);
            if !kept {
                closes.push(did_close(&told.uri));
            }
            kept
        });

        for doc in docs {
            match server.open.get_mut(&doc.id) {
                None => {
                    messages.push(did_open(doc));
                    server.open.insert(
                        doc.id,
                        Told {
                            uri: doc.uri.clone(),
                            text: doc.text.clone(),
                            version: doc.version,
                            saves: doc.saves,
                        },
                    );
                }
                Some(told) => {
                    if doc.version != told.version {
                        if let Some(change) = change_between(&told.text, &doc.text) {
                            messages.push(did_change(doc, &change.start, &change.end, change.text));
                            told.text = doc.text.clone();
                        }
                        told.version = doc.version;
                    }
                    if doc.saves != told.saves {
                        messages.push(Outgoing {
                            method: "textDocument/didSave",
                            params: json!({ "textDocument": { "uri": doc.uri } }),
                        });
                        told.saves = doc.saves;
                    }
                }
            }
        }
        closes.extend(messages);
        closes
    }

    /// Forgets what every server was told, for when they were all ended.
    pub(crate) fn clear(&mut self) {
        self.servers.clear();
    }
}

fn did_open(doc: &DocSnapshot) -> Outgoing {
    Outgoing {
        method: "textDocument/didOpen",
        params: json!({
            "textDocument": {
                "uri": doc.uri,
                "languageId": doc.language_id,
                "version": doc.version,
                "text": doc.text.to_string(),
            },
        }),
    }
}

fn did_change(doc: &DocSnapshot, start: &LspPosition, end: &LspPosition, text: String) -> Outgoing {
    Outgoing {
        method: "textDocument/didChange",
        params: json!({
            "textDocument": { "uri": doc.uri, "version": doc.version },
            "contentChanges": [{
                "range": { "start": position(start), "end": position(end) },
                "text": text,
            }],
        }),
    }
}

fn did_close(uri: &str) -> Outgoing {
    Outgoing {
        method: "textDocument/didClose",
        params: json!({ "textDocument": { "uri": uri } }),
    }
}

fn position(position: &LspPosition) -> Value {
    json!({ "line": position.line, "character": position.character })
}

/// The language a file is, as a server names it, going by its extension.
pub(crate) fn language_id(extension: &str) -> String {
    match extension.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "toml" => "toml",
        "json" => "json",
        "md" | "markdown" => "markdown",
        "nix" => "nix",
        "py" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "c" => "c",
        "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "go" => "go",
        "zig" => "zig",
        "lua" => "lua",
        "sh" | "bash" => "shellscript",
        "yaml" | "yml" => "yaml",
        other => return other.to_owned(),
    }
    .to_owned()
}
