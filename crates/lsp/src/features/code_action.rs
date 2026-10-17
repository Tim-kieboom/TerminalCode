//! Code actions: what a server offers to do at a place (a quick fix for a
//! problem, a refactoring): `textDocument/codeAction` lists them, an action
//! without its edit yet is completed by `codeAction/resolve`, and a command
//! the action carries is run by `workspace/executeCommand`. The edit is the
//! same `WorkspaceEdit` a rename answers with.

use serde_json::{Value, json};

use crate::diagnostics::Severity;
use crate::position::LspPosition;
use crate::rename::{WorkspaceEdit, parse_workspace_edit};
use crate::{Connection, LspError, PendingReply};

const REQUEST: &str = "textDocument/codeAction";
const RESOLVE: &str = "codeAction/resolve";

/// A command the server runs when asked to (`workspace/executeCommand`).
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub title: String,
    pub command: String,
    pub arguments: Vec<Value>,
}

/// One thing the server offers to do.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeAction {
    pub title: String,
    /// `quickfix`, `refactor.extract`, and so on, when the server says.
    pub kind: Option<String>,
    /// What to change; `None` until the action is resolved, when the server
    /// leaves the work for then.
    pub edit: Option<WorkspaceEdit>,
    pub command: Option<Command>,
    /// Why the action cannot be done now, when the server lists it anyway.
    pub disabled: Option<String>,
    /// The one the server thinks is the right one.
    pub preferred: bool,
    /// The item as the server sent it, which is what `codeAction/resolve` takes.
    pub raw: Value,
}

/// A problem the action is asked for, as the context of the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDiagnostic {
    pub start: LspPosition,
    pub end: LspPosition,
    pub severity: Severity,
    pub message: String,
    pub source: Option<String>,
    pub code: Option<String>,
}

/// Whether the capabilities of a server say it answers `textDocument/codeAction`.
pub fn offered(capabilities: &Value) -> bool {
    match capabilities.get("codeActionProvider") {
        Some(Value::Bool(offered)) => *offered,
        Some(Value::Object(_)) => true,
        _ => false,
    }
}

/// Whether the server completes actions with `codeAction/resolve`.
pub fn resolve_offered(capabilities: &Value) -> bool {
    capabilities
        .get("codeActionProvider")
        .and_then(|provider| provider.get("resolveProvider"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Whether the server runs commands.
pub fn execute_offered(capabilities: &Value) -> bool {
    capabilities.get("executeCommandProvider").is_some()
}

/// A `textDocument/codeAction` request that was sent and has not been answered yet.
pub struct PendingActions(PendingReply);

impl PendingActions {
    /// The actions if the answer has come, without waiting.
    pub fn try_take(&mut self) -> Option<Result<Vec<CodeAction>, LspError>> {
        let answer = self.0.try_take()?;
        Some(answer.and_then(|value| parse_code_actions(&value)))
    }
}

/// A `codeAction/resolve` request that was sent and has not been answered yet.
pub struct PendingActionResolve(PendingReply);

impl PendingActionResolve {
    /// The completed action if the answer has come, without waiting.
    pub fn try_take(&mut self) -> Option<Result<CodeAction, LspError>> {
        let answer = self.0.try_take()?;
        Some(answer.and_then(|value| {
            parse_code_action(&value).ok_or_else(|| LspError::Unexpected {
                method: RESOLVE.to_owned(),
                detail: "it is not a code action".to_owned(),
            })
        }))
    }
}

/// A `workspace/executeCommand` request that was sent and has not been answered yet.
pub struct PendingCommand(PendingReply);

impl PendingCommand {
    /// Whether the command ran, if the answer has come, without waiting.
    pub fn try_take(&mut self) -> Option<Result<(), LspError>> {
        Some(self.0.try_take()?.map(drop))
    }
}

/// Asks what can be done at the range `start..end` of the document `uri`,
/// given the problems that touch it.
pub fn request(
    connection: &Connection,
    uri: &str,
    start: LspPosition,
    end: LspPosition,
    diagnostics: &[ContextDiagnostic],
) -> Result<PendingActions, LspError> {
    let diagnostics: Vec<Value> = diagnostics.iter().map(diagnostic_json).collect();
    let params = json!({
        "textDocument": { "uri": uri },
        "range": range_json(start, end),
        // Asked for by the user (not triggered automatically).
        "context": { "diagnostics": diagnostics, "triggerKind": 1 },
    });
    connection.request(REQUEST, params).map(PendingActions)
}

/// Asks the server to complete `action` (its edit, most often).
pub fn request_resolve(
    connection: &Connection,
    action: &CodeAction,
) -> Result<PendingActionResolve, LspError> {
    connection
        .request(RESOLVE, action.raw.clone())
        .map(PendingActionResolve)
}

/// Asks the server to run `command`.
pub fn request_execute(
    connection: &Connection,
    command: &Command,
) -> Result<PendingCommand, LspError> {
    let params = json!({ "command": command.command, "arguments": command.arguments });
    connection
        .request("workspace/executeCommand", params)
        .map(PendingCommand)
}

fn range_json(start: LspPosition, end: LspPosition) -> Value {
    json!({
        "start": { "line": start.line, "character": start.character },
        "end": { "line": end.line, "character": end.character },
    })
}

fn diagnostic_json(diagnostic: &ContextDiagnostic) -> Value {
    let severity = match diagnostic.severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Information => 3,
        Severity::Hint => 4,
    };
    let mut value = json!({
        "range": range_json(diagnostic.start, diagnostic.end),
        "severity": severity,
        "message": diagnostic.message,
    });
    if let Some(source) = &diagnostic.source {
        value["source"] = json!(source);
    }
    if let Some(code) = &diagnostic.code {
        value["code"] = json!(code);
    }
    value
}

/// Reads the answer to `textDocument/codeAction`: `null` or a list of code
/// actions and bare commands. An item that is neither is left out.
pub fn parse_code_actions(result: &Value) -> Result<Vec<CodeAction>, LspError> {
    match result {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => Ok(items.iter().filter_map(parse_code_action).collect()),
        _ => Err(LspError::Unexpected {
            method: REQUEST.to_owned(),
            detail: "it is neither a list nor null".to_owned(),
        }),
    }
}

/// A code action, or a bare command (which has a `command` that is a string).
pub fn parse_code_action(item: &Value) -> Option<CodeAction> {
    let title = item.get("title")?.as_str()?.to_owned();
    let edit = match item.get("edit") {
        Some(edit) => Some(parse_workspace_edit(edit).ok()?),
        None => None,
    };
    let command = match item.get("command") {
        // A bare command: the item is the command.
        Some(Value::String(name)) => Some(Command {
            title: title.clone(),
            command: name.clone(),
            arguments: arguments(item),
        }),
        Some(Value::Object(_)) => {
            let command = item.get("command")?;
            Some(Command {
                title: command
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or(&title)
                    .to_owned(),
                command: command.get("command")?.as_str()?.to_owned(),
                arguments: arguments(command),
            })
        }
        _ => None,
    };
    Some(CodeAction {
        title,
        kind: item.get("kind").and_then(Value::as_str).map(str::to_owned),
        edit,
        command,
        disabled: item
            .pointer("/disabled/reason")
            .and_then(Value::as_str)
            .map(str::to_owned),
        preferred: item
            .get("isPreferred")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        raw: item.clone(),
    })
}

fn arguments(command: &Value) -> Vec<Value> {
    command
        .get("arguments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}
