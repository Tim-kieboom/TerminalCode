//! Renaming a symbol: `textDocument/prepareRename` says whether there is a
//! symbol at a position and which text is its name, `textDocument/rename`
//! answers with a `WorkspaceEdit`: the text edits that change every use of it.
//! The positions stay as the server sent them; files are told by their URI.

use serde_json::{Value, json};

use crate::position::LspPosition;
use crate::{Connection, LspError, PendingReply};

const PREPARE: &str = "textDocument/prepareRename";
const RENAME: &str = "textDocument/rename";

/// One stretch of text to replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameEdit {
    pub start: LspPosition,
    pub end: LspPosition,
    pub new_text: String,
}

/// The edits to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEdits {
    pub uri: String,
    pub edits: Vec<RenameEdit>,
}

/// What a rename asks to be done to the files of the project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceEdit {
    /// The text edits, one entry for each file (a file named twice is one).
    pub files: Vec<FileEdits>,
    /// Operations on files themselves (`create`, `rename`, `delete`), each as
    /// text for a message, such as `rename src/a.rs`.
    pub operations: Vec<String>,
}

/// The name the server found at the position of a `prepareRename`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    /// The range of the name, when the server says; else the word at the position.
    pub range: Option<(LspPosition, LspPosition)>,
    /// The text to start the new name from, when the server says.
    pub placeholder: Option<String>,
}

/// Whether the capabilities of a server say it answers `textDocument/rename`.
pub fn offered(capabilities: &Value) -> bool {
    match capabilities.get("renameProvider") {
        Some(Value::Bool(offered)) => *offered,
        Some(Value::Object(_)) => true,
        _ => false,
    }
}

/// Whether the server also answers `textDocument/prepareRename`.
pub fn prepare_offered(capabilities: &Value) -> bool {
    capabilities
        .get("renameProvider")
        .and_then(|provider| provider.get("prepareProvider"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// A `prepareRename` request that was sent and has not been answered yet.
pub struct PendingPrepare(PendingReply);

impl PendingPrepare {
    /// The answer if it has come, without waiting: `None` while it has not;
    /// `Some(Ok(None))` when there is nothing to rename there.
    pub fn try_take(&mut self) -> Option<Result<Option<Prepared>, LspError>> {
        let answer = self.0.try_take()?;
        Some(answer.and_then(|value| parse_prepare(&value)))
    }
}

/// A `rename` request that was sent and has not been answered yet.
pub struct PendingRename(PendingReply);

impl PendingRename {
    /// The edit if the answer has come, without waiting.
    pub fn try_take(&mut self) -> Option<Result<WorkspaceEdit, LspError>> {
        let answer = self.0.try_take()?;
        Some(answer.and_then(|value| parse_workspace_edit(&value)))
    }
}

/// Asks whether there is a symbol at `position` of `uri` that can be renamed.
pub fn request_prepare(
    connection: &Connection,
    uri: &str,
    position: LspPosition,
) -> Result<PendingPrepare, LspError> {
    let params = json!({
        "textDocument": { "uri": uri },
        "position": { "line": position.line, "character": position.character },
    });
    connection.request(PREPARE, params).map(PendingPrepare)
}

/// Asks for the edits that rename the symbol at `position` of `uri` to `new_name`.
pub fn request_rename(
    connection: &Connection,
    uri: &str,
    position: LspPosition,
    new_name: &str,
) -> Result<PendingRename, LspError> {
    let params = json!({
        "textDocument": { "uri": uri },
        "position": { "line": position.line, "character": position.character },
        "newName": new_name,
    });
    connection.request(RENAME, params).map(PendingRename)
}

/// Reads the answer to `prepareRename`: nothing (`null`), a range, a range with
/// a placeholder, or `{ "defaultBehavior": true }` (the word at the position).
pub fn parse_prepare(result: &Value) -> Result<Option<Prepared>, LspError> {
    match result {
        Value::Null => Ok(None),
        Value::Object(fields) if fields.contains_key("start") => Ok(Some(Prepared {
            range: Some(range(result).ok_or_else(|| unexpected(PREPARE, "a broken range"))?),
            placeholder: None,
        })),
        Value::Object(fields) if fields.contains_key("range") => Ok(Some(Prepared {
            range: Some(
                range(&fields["range"]).ok_or_else(|| unexpected(PREPARE, "a broken range"))?,
            ),
            placeholder: fields
                .get("placeholder")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })),
        Value::Object(fields) if fields.contains_key("defaultBehavior") => Ok(Some(Prepared {
            range: None,
            placeholder: None,
        })),
        _ => Err(unexpected(
            PREPARE,
            "it is neither a range, a range with a placeholder nor null",
        )),
    }
}

/// Reads the answer to `rename`: `null` (nothing to change), or a
/// `WorkspaceEdit` with `changes` (a map from URI to edits) and/or
/// `documentChanges` (text document edits, and file operations).
pub fn parse_workspace_edit(result: &Value) -> Result<WorkspaceEdit, LspError> {
    let mut edit = WorkspaceEdit::default();
    let fields = match result {
        Value::Null => return Ok(edit),
        Value::Object(fields) => fields,
        _ => return Err(unexpected(RENAME, "it is neither an edit nor null")),
    };

    if let Some(Value::Object(changes)) = fields.get("changes") {
        for (uri, edits) in changes {
            add_edits(&mut edit, uri, edits)?;
        }
    }
    if let Some(Value::Array(changes)) = fields.get("documentChanges") {
        for change in changes {
            match change.get("kind").and_then(Value::as_str) {
                Some(kind) => edit.operations.push(operation(kind, change)),
                None => {
                    let uri = change
                        .pointer("/textDocument/uri")
                        .and_then(Value::as_str)
                        .ok_or_else(|| unexpected(RENAME, "a document edit without a document"))?;
                    let edits = change
                        .get("edits")
                        .ok_or_else(|| unexpected(RENAME, "a document edit without edits"))?;
                    add_edits(&mut edit, uri, edits)?;
                }
            }
        }
    }
    Ok(edit)
}

/// What an operation on a file does, for a message.
fn operation(kind: &str, change: &Value) -> String {
    let name = |key: &str| change.get(key).and_then(Value::as_str).unwrap_or("?");
    match kind {
        "rename" => format!("rename {} to {}", name("oldUri"), name("newUri")),
        other => format!("{other} {}", name("uri")),
    }
}

fn add_edits(edit: &mut WorkspaceEdit, uri: &str, edits: &Value) -> Result<(), LspError> {
    let parsed = parse_text_edits(RENAME, edits)?;
    match edit.files.iter_mut().find(|file| file.uri == uri) {
        Some(file) => file.edits.extend(parsed),
        None => edit.files.push(FileEdits {
            uri: uri.to_owned(),
            edits: parsed,
        }),
    }
    Ok(())
}

/// Reads a list of text edits (`range`, `newText`) the answer to `method` has.
pub(crate) fn parse_text_edits(method: &str, edits: &Value) -> Result<Vec<RenameEdit>, LspError> {
    let Value::Array(edits) = edits else {
        return Err(unexpected(method, "edits that are not a list"));
    };
    let mut parsed = Vec::with_capacity(edits.len());
    for item in edits {
        let (start, end) = item
            .get("range")
            .and_then(range)
            .ok_or_else(|| unexpected(method, "an edit without a range"))?;
        let new_text = item
            .get("newText")
            .and_then(Value::as_str)
            .ok_or_else(|| unexpected(method, "an edit without text"))?;
        parsed.push(RenameEdit {
            start,
            end,
            new_text: new_text.to_owned(),
        });
    }
    Ok(parsed)
}

fn range(value: &Value) -> Option<(LspPosition, LspPosition)> {
    Some((position(value.get("start")?)?, position(value.get("end")?)?))
}

fn position(value: &Value) -> Option<LspPosition> {
    Some(LspPosition {
        line: u32::try_from(value.get("line")?.as_u64()?).ok()?,
        character: u32::try_from(value.get("character")?.as_u64()?).ok()?,
    })
}

fn unexpected(method: &str, detail: &str) -> LspError {
    LspError::Unexpected {
        method: method.to_owned(),
        detail: detail.to_owned(),
    }
}
