//! JSON-RPC 2.0 messages as the protocol uses them.

use serde_json::{Map, Value, json};
use thiserror::Error;

/// Answered with this when a server asks for something we do not do.
pub(crate) const METHOD_NOT_FOUND: i64 = -32601;

/// What ties an answer to its request: a number or a string, whichever the
/// sender chose.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Id {
    Number(i64),
    String(String),
}

impl Id {
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Number(number) => number.as_i64().map(Self::Number),
            Value::String(text) => Some(Self::String(text.clone())),
            _ => None,
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Self::Number(number) => json!(number),
            Self::String(text) => json!(text),
        }
    }
}

/// The error half of an answer.
#[derive(Debug, Clone, PartialEq, Error)]
#[error("{message} (code {code})")]
pub(crate) struct RpcError {
    pub(crate) code: i64,
    pub(crate) message: String,
    pub(crate) data: Option<Value>,
}

impl RpcError {
    pub(crate) fn method_not_found(method: &str) -> Self {
        Self {
            code: METHOD_NOT_FOUND,
            message: format!("{method} is not supported by this client"),
            data: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Message {
    /// Wants an answer.
    Request {
        id: Id,
        method: String,
        params: Value,
    },
    /// Wants none.
    Notification { method: String, params: Value },
    Response {
        id: Id,
        outcome: Result<Value, RpcError>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum MessageError {
    #[error("a message is not a JSON object")]
    NotAnObject,
    #[error("a message has an id that is not a number or a string")]
    BadId,
    /// An answer with no id, which is how a server reports a request it could
    /// not even read; there is nobody to give it to.
    #[error("an answer has no id to give it to")]
    Unroutable,
    #[error("a message is neither a request, a notification nor an answer")]
    Unknown,
}

impl Message {
    pub(crate) fn from_value(value: Value) -> Result<Self, MessageError> {
        let Value::Object(mut object) = value else {
            return Err(MessageError::NotAnObject);
        };
        let id = match object.remove("id") {
            None | Some(Value::Null) => None,
            Some(id) => Some(Id::from_value(&id).ok_or(MessageError::BadId)?),
        };
        let params = object.remove("params").unwrap_or(Value::Null);

        if let Some(Value::String(method)) = object.remove("method") {
            return Ok(match id {
                Some(id) => Self::Request { id, method, params },
                None => Self::Notification { method, params },
            });
        }

        let outcome = match (object.remove("error"), object.remove("result")) {
            (Some(error), _) => Err(error_from(error)),
            (None, Some(result)) => Ok(result),
            (None, None) => return Err(MessageError::Unknown),
        };
        let id = id.ok_or(MessageError::Unroutable)?;
        Ok(Self::Response { id, outcome })
    }

    pub(crate) fn to_value(&self) -> Value {
        let mut object = Map::new();
        object.insert("jsonrpc".to_owned(), json!("2.0"));
        match self {
            Self::Request { id, method, params } => {
                object.insert("id".to_owned(), id.to_value());
                object.insert("method".to_owned(), json!(method));
                insert_params(&mut object, params);
            }
            Self::Notification { method, params } => {
                object.insert("method".to_owned(), json!(method));
                insert_params(&mut object, params);
            }
            Self::Response { id, outcome } => {
                object.insert("id".to_owned(), id.to_value());
                match outcome {
                    Ok(result) => object.insert("result".to_owned(), result.clone()),
                    Err(error) => object.insert("error".to_owned(), error_to_value(error)),
                };
            }
        }
        Value::Object(object)
    }
}

fn insert_params(object: &mut Map<String, Value>, params: &Value) {
    if !params.is_null() {
        object.insert("params".to_owned(), params.clone());
    }
}

fn error_from(value: Value) -> RpcError {
    let Value::Object(mut object) = value else {
        return RpcError {
            code: 0,
            message: value.to_string(),
            data: None,
        };
    };
    RpcError {
        code: object.get("code").and_then(Value::as_i64).unwrap_or(0),
        message: match object.remove("message") {
            Some(Value::String(message)) => message,
            _ => "no message".to_owned(),
        },
        data: object.remove("data"),
    }
}

fn error_to_value(error: &RpcError) -> Value {
    let mut object = Map::new();
    object.insert("code".to_owned(), json!(error.code));
    object.insert("message".to_owned(), json!(error.message));
    if let Some(data) = &error.data {
        object.insert("data".to_owned(), data.clone());
    }
    Value::Object(object)
}
