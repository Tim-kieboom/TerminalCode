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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToPlugin {
    /// The plugin was loaded, or started again after it trapped: draw.
    Start,
    /// Something changed that the plugin may want to draw again.
    Refresh,
    /// One of the plugin's actions was run (from a key, the palette or a
    /// binding of the user's): `action` is the `id` of its [`ActionDecl`].
    Action { action: String },
    /// Text was typed (or pasted) into the plugin's input view (see
    /// [`WireView::Input`]). Only sent while that view has the keyboard.
    Text { text: String },
    /// A key that is not text was pressed in the plugin's input view: `key` is
    /// `backspace`, `delete`, `enter`, `tab`, `left`, `right`, `up`, `down`,
    /// `home`, `end`, `page_up` or `page_down`, or a single character when
    /// `ctrl` or `alt` was held with it.
    Key {
        key: String,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        shift: bool,
    },
    /// The user pressed `esc` in the input view: the editor takes the keyboard
    /// back. The plugin should stop showing the input.
    InputEnded,
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
    /// A line of text the user types into: the prompt, then the text the plugin
    /// holds, with the cursor at character `cursor` of it. While a pane shows
    /// this and has the keyboard, every key goes to the plugin as [`ToPlugin::Text`]
    /// or [`ToPlugin::Key`] (except `esc`, which the editor keeps for itself and
    /// answers with [`ToPlugin::InputEnded`]), and the plugin sends a new view for
    /// every change.
    Input {
        #[serde(default)]
        prompt: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        cursor: usize,
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

impl ToPlugin {
    pub fn action(action: impl Into<String>) -> Self {
        Self::Action {
            action: action.into(),
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn key(key: impl Into<String>, ctrl: bool, alt: bool, shift: bool) -> Self {
        Self::Key {
            key: key.into(),
            ctrl,
            alt,
            shift,
        }
    }
}

/// An action a plugin has: it shows in the command palette and can have keys.
/// The manifest lists them; the editor sends [`ToPlugin::Action`] when one runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ActionDecl {
    /// What the plugin knows it by; no `:` in it.
    pub id: String,
    /// What the palette shows after the plugin's name.
    #[serde(default)]
    pub title: String,
    /// Keys that run it while the plugin's pane has the keyboard, such as `s` or
    /// `ctrl+k ctrl+s`, which the user can change in their keymap.
    #[serde(default)]
    pub keys: Option<String>,
}

impl ActionDecl {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            keys: None,
        }
    }

    /// The same action with default keys.
    #[must_use]
    pub fn with_keys(mut self, keys: impl Into<String>) -> Self {
        self.keys = Some(keys.into());
        self
    }
}

impl WireView {
    pub fn input(prompt: impl Into<String>, value: impl Into<String>, cursor: usize) -> Self {
        Self::Input {
            prompt: prompt.into(),
            value: value.into(),
            cursor,
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
