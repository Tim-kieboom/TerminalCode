//! Asking a server what it knows about the thing at a position (its type, its
//! signature, its documentation) and reading the answer as plain text.

use serde_json::{Value, json};

use crate::docs::plain_text;
use crate::position::LspPosition;
use crate::{Connection, LspError, PendingReply};

const METHOD: &str = "textDocument/hover";

/// Whether the capabilities of a server say it answers `textDocument/hover`.
pub fn offered(capabilities: &Value) -> bool {
    match capabilities.get("hoverProvider") {
        Some(Value::Bool(offered)) => *offered,
        Some(Value::Object(_)) => true,
        _ => false,
    }
}

/// A hover request that was sent and has not been answered yet.
pub struct PendingHover(PendingReply);

impl PendingHover {
    /// The text if the answer has come, without waiting: `None` while it has
    /// not; `Some(Ok(None))` when the server has nothing to say.
    pub fn try_take(&mut self) -> Option<Result<Option<String>, LspError>> {
        let answer = self.0.try_take()?;
        Some(answer.and_then(|value| parse_hover(&value)))
    }
}

/// Asks the server what it can say about `position` of the document `uri`.
pub fn request(
    connection: &Connection,
    uri: &str,
    position: LspPosition,
) -> Result<PendingHover, LspError> {
    let params = json!({
        "textDocument": { "uri": uri },
        "position": { "line": position.line, "character": position.character },
    });
    connection.request(METHOD, params).map(PendingHover)
}

/// Reads the answer to `textDocument/hover` as plain text: `null`, or nothing
/// worth showing, is `None`. The contents are a string, a block of code
/// (`{ language, value }`), markup (`{ kind, value }`) or a list of them.
pub fn parse_hover(result: &Value) -> Result<Option<String>, LspError> {
    let contents = match result {
        Value::Null => return Ok(None),
        Value::Object(hover) => hover.get("contents").ok_or_else(|| LspError::Unexpected {
            method: METHOD.to_owned(),
            detail: "it has no contents".to_owned(),
        })?,
        _ => {
            return Err(LspError::Unexpected {
                method: METHOD.to_owned(),
                detail: "it is neither an object nor null".to_owned(),
            });
        }
    };
    let parts: Vec<&Value> = match contents {
        Value::Array(parts) => parts.iter().collect(),
        single => vec![single],
    };
    let text = parts
        .into_iter()
        .filter_map(part_text)
        .collect::<Vec<_>>()
        .join("\n\n");
    let text = plain_text(&text);
    Ok((!text.is_empty()).then_some(text))
}

/// The text of one piece of contents; a block of code keeps its text as it is.
fn part_text(part: &Value) -> Option<String> {
    match part {
        Value::String(text) => Some(text.clone()),
        Value::Object(object) => object.get("value")?.as_str().map(str::to_owned),
        _ => None,
    }
}
