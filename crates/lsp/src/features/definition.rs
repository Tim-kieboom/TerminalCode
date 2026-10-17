//! Asking a server where the thing at a position is defined, and reading the
//! answer: one location, a list of them, or links (a location with the range
//! of the name). The positions stay as the server sent them; the file is told
//! by its URI.

use serde_json::{Value, json};

use crate::position::LspPosition;
use crate::{Connection, LspError, PendingReply};

const METHOD: &str = "textDocument/definition";

/// Where something is defined: a document and the range of the name in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub start: LspPosition,
    pub end: LspPosition,
}

/// Whether the capabilities of a server say it answers `textDocument/definition`.
pub fn offered(capabilities: &Value) -> bool {
    match capabilities.get("definitionProvider") {
        Some(Value::Bool(offered)) => *offered,
        Some(Value::Object(_)) => true,
        _ => false,
    }
}

/// A definition request that was sent and has not been answered yet.
pub struct PendingDefinition(PendingReply);

impl PendingDefinition {
    /// The locations if the answer has come, without waiting: `None` while it
    /// has not; an empty list when the server has none.
    pub fn try_take(&mut self) -> Option<Result<Vec<Location>, LspError>> {
        let answer = self.0.try_take()?;
        Some(answer.and_then(|value| parse_definition(&value)))
    }
}

/// Asks the server where the thing at `position` of the document `uri` is defined.
pub fn request(
    connection: &Connection,
    uri: &str,
    position: LspPosition,
) -> Result<PendingDefinition, LspError> {
    let params = json!({
        "textDocument": { "uri": uri },
        "position": { "line": position.line, "character": position.character },
    });
    connection.request(METHOD, params).map(PendingDefinition)
}

/// Reads the answer to `textDocument/definition`: nothing (`null`), a location,
/// or a list of locations or links. A place that is not usable (no URI, no
/// range) is left out; an answer of any other shape is an error. The same place
/// twice is one.
pub fn parse_definition(result: &Value) -> Result<Vec<Location>, LspError> {
    let items: Vec<&Value> = match result {
        Value::Null => return Ok(Vec::new()),
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![result],
        _ => {
            return Err(LspError::Unexpected {
                method: METHOD.to_owned(),
                detail: "it is neither a location, a list nor null".to_owned(),
            });
        }
    };
    let mut found: Vec<Location> = Vec::new();
    for location in items.into_iter().filter_map(parse_location) {
        if !found.contains(&location) {
            found.push(location);
        }
    }
    Ok(found)
}

/// A `Location` (`uri`, `range`) or a `LocationLink` (`targetUri`, and the range
/// of the name in `targetSelectionRange`, else `targetRange`).
fn parse_location(value: &Value) -> Option<Location> {
    let (uri, range) = match value.get("targetUri") {
        Some(uri) => (
            uri.as_str()?,
            value
                .get("targetSelectionRange")
                .or_else(|| value.get("targetRange"))?,
        ),
        None => (value.get("uri")?.as_str()?, value.get("range")?),
    };
    Some(Location {
        uri: uri.to_owned(),
        start: position(range.get("start")?)?,
        end: position(range.get("end")?)?,
    })
}

fn position(value: &Value) -> Option<LspPosition> {
    Some(LspPosition {
        line: u32::try_from(value.get("line")?.as_u64()?).ok()?,
        character: u32::try_from(value.get("character")?.as_u64()?).ok()?,
    })
}
