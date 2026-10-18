//! The messages: JSON, one object each, with a `type` field.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::MAX_MESSAGE;

/// Why a message could not be written or read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MessageError {
    #[error("a message of {0} bytes, more than the {MAX_MESSAGE} allowed")]
    TooBig(usize),
    #[error("not a message: {0}")]
    Invalid(String),
}

/// What the editor tells a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToPlugin {
    /// The plugin was loaded, or started again after it trapped: draw.
    Start,
    /// Something changed that the plugin may want to draw again.
    Refresh,
    /// A kind of message a later version of the editor sends.
    #[serde(other)]
    Unknown,
}

/// What a plugin tells the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum FromPlugin {
    /// Replace what the plugin's pane shows.
    SetView { view: WireView },
    /// A kind of message a later version of the plugin API has.
    #[serde(other)]
    Unknown,
}

/// What a plugin's pane shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum WireView {
    /// Lines of text, each in the style of a theme slot.
    Lines {
        #[serde(default)]
        lines: Vec<WireLine>,
    },
    /// A list with one row highlighted.
    List {
        #[serde(default)]
        items: Vec<String>,
        #[serde(default)]
        selected: Option<usize>,
    },
    /// A kind of view a later version of the plugin API has.
    #[serde(other)]
    Unknown,
}

/// One line of text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WireLine {
    #[serde(default)]
    pub text: String,
    /// The theme slot that styles it; a slot the theme does not have is plain text.
    #[serde(default)]
    pub slot: String,
}

impl WireLine {
    pub fn new(text: impl Into<String>, slot: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            slot: slot.into(),
        }
    }
}

impl WireView {
    pub fn lines(lines: impl IntoIterator<Item = WireLine>) -> Self {
        Self::Lines {
            lines: lines.into_iter().collect(),
        }
    }

    pub fn list(items: impl IntoIterator<Item = String>, selected: Option<usize>) -> Self {
        Self::List {
            items: items.into_iter().collect(),
            selected,
        }
    }
}

impl FromPlugin {
    pub fn set_view(view: WireView) -> Self {
        Self::SetView { view }
    }
}

macro_rules! bytes {
    ($message:ty) => {
        impl $message {
            /// The message as the bytes to send.
            pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
                let bytes = serde_json::to_vec(self)
                    .map_err(|error| MessageError::Invalid(error.to_string()))?;
                match bytes.len() > MAX_MESSAGE {
                    true => Err(MessageError::TooBig(bytes.len())),
                    false => Ok(bytes),
                }
            }

            /// Reads a message that was sent. A kind of message that is not known
            /// is `Unknown`, not an error; one that is not JSON, or has no `type`,
            /// is an error.
            pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
                if bytes.len() > MAX_MESSAGE {
                    return Err(MessageError::TooBig(bytes.len()));
                }
                serde_json::from_slice(bytes)
                    .map_err(|error| MessageError::Invalid(error.to_string()))
            }
        }
    };
}

bytes!(ToPlugin);
bytes!(FromPlugin);
