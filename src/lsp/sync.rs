//! Telling a server about the documents it serves: `didOpen` when it first
//! sees one, `didChange` with what changed since it last looked, `didSave` after
//! a save and `didClose` when the document goes away. It decides what to send;
//! the app sends it.
//!
//! The state is a copy of each document as the server has it (a rope clone,
//! which shares its text), so a change is worked out against what the server
//! really knows, whatever happened to the text in between. A short history of
//! the texts the server was sent lets a late answer about an older version be
//! placed in the text as it is now.

use std::collections::{HashMap, VecDeque};
use std::ops::Range;

use ropey::Rope;
use serde_json::{Value, json};

use super::config::ServerKey;
use super::diagnostics::map_range;
use super::diff::{ByteChange, change_between};
use super::position::{LspPosition, from_lsp};
use crate::components::workspace::DocumentId;

/// How many of the texts a server was sent are kept, for placing an answer
/// that is about an older one.
const HISTORY: usize = 32;

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
    /// Grows every time the text is read again from the file.
    pub(crate) reloads: u64,
}

/// A message for the server.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Outgoing {
    pub(crate) method: &'static str,
    pub(crate) params: Value,
}

/// What a pass of the sync found: what to send, and what happened to the text
/// that anything pointing into it has to follow.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct SyncOutput {
    pub(crate) messages: Vec<Outgoing>,
    /// Changes to the text the server was told, in order, by document.
    pub(crate) changes: Vec<(DocumentId, ByteChange)>,
    /// Documents whose text is no longer the one anything pointed into: new to
    /// the server, closed, read again from the file, or served by a server
    /// that started again.
    pub(crate) resets: Vec<DocumentId>,
}

/// A text the server was sent, and how it came from the one before.
#[derive(Debug)]
struct Sent {
    version: u64,
    text: Rope,
    /// From the previous text; none for the first.
    change: Option<ByteChange>,
}

/// What a server has been told about a document.
#[derive(Debug)]
struct Told {
    uri: String,
    text: Rope,
    version: u64,
    saves: u64,
    reloads: u64,
    history: VecDeque<Sent>,
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
        self.update(key, generation, docs).messages
    }

    /// Like [`DocumentSync::messages`], and says what happened to the texts too.
    pub(crate) fn update(
        &mut self,
        key: &ServerKey,
        generation: u64,
        docs: &[DocSnapshot],
    ) -> SyncOutput {
        let server = self.servers.entry(key.clone()).or_default();
        let mut output = SyncOutput::default();
        if server.generation != generation {
            // A server that started again has nothing open.
            output.resets.extend(server.open.keys().copied());
            *server = ServerDocs {
                generation,
                open: HashMap::new(),
            };
        }

        // Documents that went away, or that now have another name.
        let mut closes = Vec::new();
        server.open.retain(|id, told| {
            let kept = docs.iter().any(|doc| doc.id == *id && doc.uri == told.uri);
            if !kept {
                closes.push(did_close(&told.uri));
                output.resets.push(*id);
            }
            kept
        });

        for doc in docs {
            let Some(told) = server.open.get_mut(&doc.id) else {
                output.messages.push(did_open(doc));
                output.resets.push(doc.id);
                server.open.insert(doc.id, Told::opened(doc));
                continue;
            };
            if doc.version != told.version {
                if let Some(change) = change_between(&told.text, &doc.text) {
                    output.messages.push(did_change(doc, &change));
                    told.text = doc.text.clone();
                    told.remember(doc.version, change.bytes);
                    match doc.reloads == told.reloads {
                        true => output.changes.push((doc.id, change.bytes)),
                        false => output.resets.push(doc.id),
                    }
                } else if doc.reloads != told.reloads {
                    output.resets.push(doc.id);
                }
                told.version = doc.version;
            } else if doc.reloads != told.reloads {
                output.resets.push(doc.id);
            }
            told.reloads = doc.reloads;
            if doc.saves != told.saves {
                output.messages.push(Outgoing {
                    method: "textDocument/didSave",
                    params: json!({ "textDocument": { "uri": doc.uri } }),
                });
                told.saves = doc.saves;
            }
        }
        closes.extend(output.messages);
        output.messages = closes;
        output
    }

    /// Where a server's answer about `document` points, in the text the server
    /// was last told. `ranges` are positions in the text of `version` (the
    /// latest the server was sent, when the answer names none); they are moved
    /// through every change sent since. A range that a change took is `None`.
    /// The whole answer is `None` when `version` is not one that is kept.
    pub(crate) fn ranges_in_told_text(
        &self,
        key: &ServerKey,
        document: DocumentId,
        version: Option<u64>,
        ranges: &[(LspPosition, LspPosition)],
    ) -> Option<Vec<Option<Range<usize>>>> {
        let told = self.servers.get(key)?.open.get(&document)?;
        let at = match version {
            None => told.history.len().checked_sub(1)?,
            Some(version) => told
                .history
                .iter()
                .position(|sent| sent.version == version)?,
        };
        let text = &told.history[at].text;
        let later: Vec<ByteChange> = told
            .history
            .iter()
            .skip(at + 1)
            .filter_map(|sent| sent.change)
            .collect();
        Some(
            ranges
                .iter()
                .map(|(start, end)| {
                    let start = from_lsp(text, *start);
                    let end = from_lsp(text, *end).max(start);
                    later.iter().try_fold(start..end, map_range)
                })
                .collect(),
        )
    }

    /// Forgets a server that is not running, and says which documents had been
    /// opened on it.
    pub(crate) fn drop_server(&mut self, key: &ServerKey) -> Vec<DocumentId> {
        self.servers
            .remove(key)
            .map(|server| server.open.into_keys().collect())
            .unwrap_or_default()
    }

    /// Whether the server `key` has been told to open `document` (and not to close it).
    pub(crate) fn has_open(&self, key: &ServerKey, document: DocumentId) -> bool {
        self.servers
            .get(key)
            .is_some_and(|server| server.open.contains_key(&document))
    }

    /// The servers that have been told about something.
    pub(crate) fn servers(&self) -> Vec<ServerKey> {
        self.servers.keys().cloned().collect()
    }

    /// Forgets what every server was told, for when they were all ended.
    pub(crate) fn clear(&mut self) {
        self.servers.clear();
    }
}

impl Told {
    fn opened(doc: &DocSnapshot) -> Self {
        let mut history = VecDeque::new();
        history.push_back(Sent {
            version: doc.version,
            text: doc.text.clone(),
            change: None,
        });
        Self {
            uri: doc.uri.clone(),
            text: doc.text.clone(),
            version: doc.version,
            saves: doc.saves,
            reloads: doc.reloads,
            history,
        }
    }

    /// Notes that the text `self.text` was sent as `version`, by `change`.
    fn remember(&mut self, version: u64, change: ByteChange) {
        self.history.push_back(Sent {
            version,
            text: self.text.clone(),
            change: Some(change),
        });
        while self.history.len() > HISTORY {
            self.history.pop_front();
        }
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

fn did_change(doc: &DocSnapshot, change: &super::diff::Change) -> Outgoing {
    Outgoing {
        method: "textDocument/didChange",
        params: json!({
            "textDocument": { "uri": doc.uri, "version": doc.version },
            "contentChanges": [{
                "range": { "start": position(&change.start), "end": position(&change.end) },
                "text": change.text,
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
