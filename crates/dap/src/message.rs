//! The three kinds of message of the Debug Adapter Protocol, as JSON.
//!
//! ```text
//! {"seq": 3, "type": "request",  "command": "next", "arguments": {"threadId": 1}}
//! {"seq": 8, "type": "response", "request_seq": 3, "success": true, "command": "next", "body": {}}
//! {"seq": 9, "type": "event",    "event": "stopped", "body": {"reason": "step", "threadId": 1}}
//! ```
//!
//! Each side numbers the messages it sends from 1 (`seq`); a response names the request it
//! answers by its `seq` (`request_seq`). Unlike JSON-RPC, a request that fails is a response with
//! `success: false` and a short `message`.

use std::fmt;

use serde_json::{Map, Value, json};
use thiserror::Error;

/// Why a message that was read is not one.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MessageError {
    #[error("a message is not a JSON object")]
    NotAnObject,
    #[error("a message has no `{0}` of the right kind")]
    Missing(&'static str),
    #[error("`{0}` is not a kind of message (request, response or event)")]
    Kind(String),
}

/// What a failed request says: a short `message` (such as `cancelled`, `notStopped`) and
/// the `body`, which may hold a longer `error` to show the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Rejection {
    pub message: String,
    pub body: Value,
}

impl Rejection {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            body: Value::Null,
        }
    }

    /// What to tell the user: the adapter's own words for it (`body.error.format`, with the
    /// `{name}` placeholders filled from `body.error.variables`) when it has them, else the
    /// short message.
    pub fn describe(&self) -> String {
        let error = &self.body["error"];
        let Some(format) = error["format"].as_str() else {
            return self.message.clone();
        };
        let mut text = format.to_owned();
        if let Some(variables) = error["variables"].as_object() {
            for (name, value) in variables {
                let value = value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned);
                text = text.replace(&format!("{{{name}}}"), &value);
            }
        }
        text
    }
}

impl fmt::Display for Rejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.describe())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// A request: from the client to the adapter, or (a reverse request such as
    /// `runInTerminal`) from the adapter to the client.
    Request {
        seq: i64,
        command: String,
        arguments: Value,
    },
    Response {
        seq: i64,
        /// The `seq` of the request this answers.
        request_seq: i64,
        command: String,
        outcome: Result<Value, Rejection>,
    },
    Event {
        seq: i64,
        event: String,
        body: Value,
    },
}

impl Message {
    pub fn to_value(&self) -> Value {
        match self {
            Self::Request {
                seq,
                command,
                arguments,
            } => {
                let mut object = json!({ "seq": seq, "type": "request", "command": command });
                if !arguments.is_null() {
                    object["arguments"] = arguments.clone();
                }
                object
            }
            Self::Response {
                seq,
                request_seq,
                command,
                outcome,
            } => {
                let mut object = json!({
                    "seq": seq, "type": "response", "request_seq": request_seq,
                    "command": command, "success": outcome.is_ok(),
                });
                match outcome {
                    Ok(body) if !body.is_null() => object["body"] = body.clone(),
                    Ok(_) => {}
                    Err(rejection) => {
                        object["message"] = json!(rejection.message);
                        if !rejection.body.is_null() {
                            object["body"] = rejection.body.clone();
                        }
                    }
                }
                object
            }
            Self::Event { seq, event, body } => {
                let mut object = json!({ "seq": seq, "type": "event", "event": event });
                if !body.is_null() {
                    object["body"] = body.clone();
                }
                object
            }
        }
    }

    pub fn from_value(value: Value) -> Result<Self, MessageError> {
        let Value::Object(mut object) = value else {
            return Err(MessageError::NotAnObject);
        };
        let seq = integer(&object, "seq")?;
        let kind = text(&mut object, "type")?;
        match kind.as_str() {
            "request" => Ok(Self::Request {
                seq,
                command: text(&mut object, "command")?,
                arguments: object.remove("arguments").unwrap_or(Value::Null),
            }),
            "response" => {
                let success = object
                    .get("success")
                    .and_then(Value::as_bool)
                    .ok_or(MessageError::Missing("success"))?;
                let body = object.remove("body").unwrap_or(Value::Null);
                let outcome = match success {
                    true => Ok(body),
                    false => Err(Rejection {
                        message: text(&mut object, "message").unwrap_or_default(),
                        body,
                    }),
                };
                Ok(Self::Response {
                    seq,
                    request_seq: integer(&object, "request_seq")?,
                    command: text(&mut object, "command")?,
                    outcome,
                })
            }
            "event" => Ok(Self::Event {
                seq,
                event: text(&mut object, "event")?,
                body: object.remove("body").unwrap_or(Value::Null),
            }),
            other => Err(MessageError::Kind(other.to_owned())),
        }
    }
}

fn integer(object: &Map<String, Value>, name: &'static str) -> Result<i64, MessageError> {
    object
        .get(name)
        .and_then(Value::as_i64)
        .ok_or(MessageError::Missing(name))
}

fn text(object: &mut Map<String, Value>, name: &'static str) -> Result<String, MessageError> {
    match object.remove(name) {
        Some(Value::String(text)) => Ok(text),
        _ => Err(MessageError::Missing(name)),
    }
}
