//! What a server says is wrong with a document: the diagnostics it publishes,
//! kept in byte offsets of the text, and moved through the edits made after
//! the server looked, so that they stay on the text they were about.
//!
//! The ranges always refer to the text the server has been told, which the
//! sync brings up to date before each frame; so when a frame is drawn they
//! refer to the text on screen.

use std::collections::HashMap;
use std::ops::Range;

use serde_json::Value;

use super::diff::ByteChange;
use super::position::LspPosition;
use crate::components::workspace::DocumentId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Severity {
    /// The worst first, so that sorting by severity puts errors in front.
    Error,
    Warning,
    Information,
    Hint,
}

impl Severity {
    /// A server that names none leaves it to the client; an error is the safe reading.
    fn from_code(code: Option<u64>) -> Self {
        match code {
            Some(2) => Self::Warning,
            Some(3) => Self::Information,
            Some(4) => Self::Hint,
            _ => Self::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Diagnostic {
    /// The bytes it is about; empty for a position.
    pub(crate) range: Range<usize>,
    pub(crate) severity: Severity,
    pub(crate) message: String,
    /// Which tool said it (`rustc`, `clippy`).
    pub(crate) source: Option<String>,
    pub(crate) code: Option<String>,
}

/// A diagnostic as the server sent it: positions, not bytes yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawDiagnostic {
    pub(crate) start: LspPosition,
    pub(crate) end: LspPosition,
    pub(crate) severity: Severity,
    pub(crate) message: String,
    pub(crate) source: Option<String>,
    pub(crate) code: Option<String>,
}

/// What `textDocument/publishDiagnostics` carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Published {
    pub(crate) uri: String,
    /// The version of the document the server looked at, when it says.
    pub(crate) version: Option<u64>,
    pub(crate) diagnostics: Vec<RawDiagnostic>,
}

/// Reads the `params` of `textDocument/publishDiagnostics`. A diagnostic that is
/// not usable (no range, no message) is left out; params that are not usable at all give `None`.
pub(crate) fn parse_published(params: &Value) -> Option<Published> {
    let uri = params.get("uri")?.as_str()?.to_owned();
    let version = params.get("version").and_then(Value::as_u64);
    let diagnostics = params
        .get("diagnostics")?
        .as_array()?
        .iter()
        .filter_map(parse_diagnostic)
        .collect();
    Some(Published {
        uri,
        version,
        diagnostics,
    })
}

fn parse_diagnostic(value: &Value) -> Option<RawDiagnostic> {
    let range = value.get("range")?;
    Some(RawDiagnostic {
        start: parse_position(range.get("start")?)?,
        end: parse_position(range.get("end")?)?,
        severity: Severity::from_code(value.get("severity").and_then(Value::as_u64)),
        message: value.get("message")?.as_str()?.to_owned(),
        source: value
            .get("source")
            .and_then(Value::as_str)
            .map(str::to_owned),
        // A code is a number or a string.
        code: match value.get("code") {
            Some(Value::String(code)) => Some(code.clone()),
            Some(Value::Number(code)) => Some(code.to_string()),
            _ => None,
        },
    })
}

fn parse_position(value: &Value) -> Option<LspPosition> {
    Some(LspPosition {
        line: u32::try_from(value.get("line")?.as_u64()?).ok()?,
        character: u32::try_from(value.get("character")?.as_u64()?).ok()?,
    })
}

/// Where `range` ends up after `change`, or `None` when the change took the
/// text it was about.
///
/// - An edit after the range, or one that only ends where the range starts at
///   its end, leaves it alone.
/// - An edit before it moves it.
/// - Text typed inside it (nothing deleted) makes it longer.
/// - Any edit that deletes or replaces some of it, or reaches into it, drops it.
///
/// A range with no length is a point, and it stays before text typed at it.
pub(crate) fn map_range(range: Range<usize>, change: &ByteChange) -> Option<Range<usize>> {
    let (start, end) = (range.start, range.end);
    if end <= change.start {
        return Some(range);
    }
    if start >= change.old_end {
        let moved = |at: usize| at - change.old_end + change.new_end;
        return Some(moved(start)..moved(end));
    }
    // The edit touches the inside of the range.
    let only_typed = change.old_end == change.start;
    if only_typed && start < change.start && change.start < end {
        return Some(start..end - change.old_end + change.new_end);
    }
    None
}

/// The diagnostics of every open document.
#[derive(Debug, Default)]
pub(crate) struct Diagnostics {
    documents: HashMap<DocumentId, Vec<Diagnostic>>,
}

impl Diagnostics {
    /// Replaces what a document has with `diagnostics`, in the order of the text.
    pub(crate) fn set(&mut self, document: DocumentId, mut diagnostics: Vec<Diagnostic>) {
        if diagnostics.is_empty() {
            self.documents.remove(&document);
            return;
        }
        diagnostics.sort_by_key(|diagnostic| (diagnostic.range.start, diagnostic.range.end));
        self.documents.insert(document, diagnostics);
    }

    /// Moves a document's diagnostics through `change`, dropping the ones it took.
    pub(crate) fn remap(&mut self, document: DocumentId, change: &ByteChange) {
        let Some(diagnostics) = self.documents.get_mut(&document) else {
            return;
        };
        diagnostics.retain_mut(
            |diagnostic| match map_range(diagnostic.range.clone(), change) {
                Some(range) => {
                    diagnostic.range = range;
                    true
                }
                None => false,
            },
        );
        if diagnostics.is_empty() {
            self.documents.remove(&document);
        }
    }

    pub(crate) fn clear(&mut self, document: DocumentId) {
        self.documents.remove(&document);
    }

    pub(crate) fn clear_all(&mut self) {
        self.documents.clear();
    }

    /// What a document has, in the order of the text.
    pub(crate) fn of(&self, document: DocumentId) -> &[Diagnostic] {
        self.documents.get(&document).map_or(&[], Vec::as_slice)
    }

    /// The ones that touch `bytes`: a range that ends where `bytes` starts does
    /// not, and a point counts when it is inside or at the start.
    pub(crate) fn overlapping(
        &self,
        document: DocumentId,
        bytes: Range<usize>,
    ) -> impl Iterator<Item = &Diagnostic> {
        self.of(document).iter().filter(move |diagnostic| {
            let range = &diagnostic.range;
            match range.is_empty() {
                true => range.start >= bytes.start && range.start < bytes.end.max(bytes.start + 1),
                false => range.start < bytes.end && range.end > bytes.start,
            }
        })
    }

    /// The first diagnostic that starts after `byte`, going round to the first
    /// of the document when there is none.
    pub(crate) fn next_after(&self, document: DocumentId, byte: usize) -> Option<&Diagnostic> {
        let all = self.of(document);
        all.iter()
            .find(|diagnostic| diagnostic.range.start > byte)
            .or_else(|| all.first())
    }

    /// The last diagnostic that starts before `byte`, going round to the last
    /// of the document when there is none.
    pub(crate) fn previous_before(&self, document: DocumentId, byte: usize) -> Option<&Diagnostic> {
        let all = self.of(document);
        all.iter()
            .rev()
            .find(|diagnostic| diagnostic.range.start < byte)
            .or_else(|| all.last())
    }

    /// How many a document has of each severity: errors, warnings, the rest.
    pub(crate) fn counts(&self, document: DocumentId) -> (usize, usize, usize) {
        self.of(document)
            .iter()
            .fold(
                (0, 0, 0),
                |(errors, warnings, rest), diagnostic| match diagnostic.severity {
                    Severity::Error => (errors + 1, warnings, rest),
                    Severity::Warning => (errors, warnings + 1, rest),
                    _ => (errors, warnings, rest + 1),
                },
            )
    }
}
