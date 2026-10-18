//! Formatting a document: `textDocument/formatting` answers with the text edits
//! that make the whole document as the server's formatter writes it (for Rust,
//! `rustfmt` through `rust-analyzer`, which follows the project's `rustfmt.toml`).

use serde_json::{Value, json};

use crate::rename::{RenameEdit, parse_text_edits};
use crate::{Connection, LspError, PendingReply};

const METHOD: &str = "textDocument/formatting";

/// Whether the capabilities of a server say it answers `textDocument/formatting`.
pub fn offered(capabilities: &Value) -> bool {
    match capabilities.get("documentFormattingProvider") {
        Some(Value::Bool(offered)) => *offered,
        Some(Value::Object(_)) => true,
        _ => false,
    }
}

/// A formatting request that was sent and has not been answered yet.
pub struct PendingFormat(PendingReply);

impl PendingFormat {
    /// The edits if the answer has come, without waiting: `None` while it has
    /// not; an empty list when there is nothing to change.
    pub fn try_take(&mut self) -> Option<Result<Vec<RenameEdit>, LspError>> {
        let answer = self.0.try_take()?;
        Some(answer.and_then(|value| parse_formatting(&value)))
    }
}

/// Asks the server to format the document `uri`. The options are what the
/// server uses when it has no settings of its own.
pub fn request(
    connection: &Connection,
    uri: &str,
    tab_size: u32,
    insert_spaces: bool,
) -> Result<PendingFormat, LspError> {
    let params = json!({
        "textDocument": { "uri": uri },
        "options": { "tabSize": tab_size, "insertSpaces": insert_spaces },
    });
    connection.request(METHOD, params).map(PendingFormat)
}

/// Reads the answer to `textDocument/formatting`: `null` (nothing to change) or
/// a list of edits.
pub fn parse_formatting(result: &Value) -> Result<Vec<RenameEdit>, LspError> {
    match result {
        Value::Null => Ok(Vec::new()),
        edits => parse_text_edits(METHOD, edits),
    }
}
