//! Starting and ending a session: `initialize`, `initialized`, `shutdown`, `exit`.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::uri::file_uri;
use crate::{Connection, LspError};

/// What the server told us about itself in its answer to `initialize`.
#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    /// Its name, when it gave one (`rust-analyzer`).
    pub name: Option<String>,
    /// What it can do, as it sent it.
    pub capabilities: Value,
}

impl Server {
    /// What the server offers for completion; `None` when it offers none.
    pub fn completion(&self) -> Option<crate::Provider> {
        crate::Provider::from_capabilities(&self.capabilities)
    }
}

/// What this client can do, as `initialize` says it. Only what is used: a
/// server offers features to the clients that say they take them.
pub fn client_capabilities() -> Value {
    json!({
        "general": {
            // Positions are counted in UTF-16 code units, which every server
            // knows how to do; nothing else is offered.
            "positionEncodings": ["utf-16"],
        },
        "textDocument": {
            "synchronization": {
                "dynamicRegistration": false,
                "didSave": true,
            },
            "hover": {
                "dynamicRegistration": false,
                "contentFormat": ["markdown", "plaintext"],
            },
            "definition": {
                "dynamicRegistration": false,
                // The range of the name comes with the place, not only the whole item.
                "linkSupport": true,
            },
            "completion": {
                "dynamicRegistration": false,
                // The server is told why it is asked (a typed trigger character,
                // a list that grew), and every kind of item is understood.
                "contextSupport": true,
                "completionItemKind": { "valueSet": (1..=25).collect::<Vec<u32>>() },
                // Snippets (written without their placeholders), no insert-or-replace. The edits that
                // go with an item (the `use` line of an import) are asked for when
                // it is picked; a server such as rust-analyzer offers the items that
                // are not imported yet only to a client that does that.
                "completionItem": {
                    "documentationFormat": ["markdown", "plaintext"],
                    "snippetSupport": true,
                    "insertReplaceSupport": false,
                    "labelDetailsSupport": true,
                    "resolveSupport": { "properties": ["additionalTextEdits"] },
                },
            },
            "rename": {
                "dynamicRegistration": false,
                // Whether there is a symbol to rename is asked first, which also
                // gives the name to start from.
                "prepareSupport": true,
            },
            "codeAction": {
                "dynamicRegistration": false,
                // Actions come as objects with their kind, and an action's edit
                // may be left for `codeAction/resolve`.
                "codeActionLiteralSupport": {
                    "codeActionKind": { "valueSet": [
                        "", "quickfix", "refactor", "refactor.extract", "refactor.inline",
                        "refactor.rewrite", "source", "source.organizeImports",
                    ] },
                },
                "isPreferredSupport": true,
                "disabledSupport": true,
                "dataSupport": true,
                "resolveSupport": { "properties": ["edit"] },
            },
            "publishDiagnostics": {
                "versionSupport": true,
            },
        },
        "window": {
            "workDoneProgress": true,
        },
        "workspace": {
            "workspaceFolders": true,
            // Edits come as a list of documents, and no operation on files is
            // taken (a rename that needs one is refused by the client).
            "workspaceEdit": { "documentChanges": true },
            "executeCommand": { "dynamicRegistration": false },
        },
    })
}

/// Starts the session for the project at `root`: asks the server to
/// initialize (waiting at most `timeout` for it), then says it is done.
/// `options` goes to the server as it is (`initializationOptions`); `null` sends none.
pub fn initialize(
    connection: &Connection,
    root: &Path,
    options: &Value,
    timeout: Duration,
) -> Result<Server, LspError> {
    let uri = file_uri(root).ok_or_else(|| LspError::NotAUri(root.display().to_string()))?;
    let name = root.file_name().map_or_else(
        || "project".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let mut params = json!({
        "processId": std::process::id(),
        "clientInfo": {
            "name": "trcode",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "rootUri": uri,
        "workspaceFolders": [{ "uri": uri, "name": name }],
        "capabilities": client_capabilities(),
    });
    if !options.is_null() {
        params["initializationOptions"] = options.clone();
    }

    let result = connection.request("initialize", params)?.wait(timeout)?;
    let server = parse_answer(&result)?;
    connection.notify("initialized", json!({}))?;
    Ok(server)
}

fn parse_answer(result: &Value) -> Result<Server, LspError> {
    let unexpected = |detail: &str| LspError::Unexpected {
        method: "initialize".to_owned(),
        detail: detail.to_owned(),
    };
    let capabilities = result
        .get("capabilities")
        .filter(|capabilities| capabilities.is_object())
        .ok_or_else(|| unexpected("it has no capabilities"))?;
    // Only offered one, a server that names another is not following the
    // protocol, and its positions would be read wrong.
    if let Some(encoding) = capabilities.get("positionEncoding").and_then(Value::as_str)
        && encoding != "utf-16"
    {
        return Err(LspError::Encoding(encoding.to_owned()));
    }
    let name = result
        .pointer("/serverInfo/name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(Server {
        name,
        capabilities: capabilities.clone(),
    })
}

/// Ends the session politely: `shutdown`, waiting at most `timeout` for the
/// answer, then `exit`. The `exit` is sent even when the server did not answer,
/// since it is what makes a server stop.
pub fn shutdown(connection: &Connection, timeout: Duration) -> Result<(), LspError> {
    let answered = connection
        .request("shutdown", Value::Null)
        .and_then(|reply| reply.wait(timeout));
    let exited = connection.notify("exit", Value::Null);
    answered.and(exited)?;
    Ok(())
}
