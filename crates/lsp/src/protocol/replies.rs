//! What a client says to the requests a server makes of it. A server that is
//! never answered waits, so even what we do not support gets an answer.

use serde_json::{Value, json};

use crate::message::RpcError;

/// The answer to the request `method` with `params`.
pub(crate) fn answer(method: &str, params: &Value) -> Result<Value, RpcError> {
    match method {
        // The progress of a long job (indexing) is reported with notifications
        // after the server made a token for it; agreeing is all it needs.
        "window/workDoneProgress/create" => Ok(Value::Null),
        // Nothing is registered or unregistered: the capabilities we sent are
        // all there is.
        "client/registerCapability" | "client/unregisterCapability" => Ok(Value::Null),
        // One answer per item asked, and no settings: the server uses its own.
        "workspace/configuration" => {
            let asked = params
                .get("items")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            Ok(Value::Array(vec![Value::Null; asked]))
        }
        // No folders beyond the one it was started with.
        "workspace/workspaceFolders" => Ok(Value::Null),
        // Refreshes of things we do not show.
        "workspace/diagnostic/refresh"
        | "workspace/semanticTokens/refresh"
        | "workspace/inlayHint/refresh"
        | "workspace/codeLens/refresh"
        | "workspace/inlineValue/refresh" => Ok(Value::Null),
        // Messages that wait for a choice get none.
        "window/showMessageRequest" => Ok(Value::Null),
        "window/showDocument" => Ok(json!({ "success": false })),
        "workspace/applyEdit" => Ok(json!({
            "applied": false,
            "failureReason": "this client does not apply edits from a server yet",
        })),
        other => Err(RpcError::method_not_found(other)),
    }
}
