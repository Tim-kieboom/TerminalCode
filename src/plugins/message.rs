//! The messages between the editor and a plugin: JSON, one object each, with a
//! `type` field. Changes are additive only; a type or a field this version does
//! not know is ignored (`Unknown`, and fields that are missing take a default),
//! so a plugin built against an older version keeps working. This is the shape
//! the `plugin-api` crate will give to authors.

use serde::{Deserialize, Serialize};

use crate::ui::view::{ViewLine, ViewNode};

/// What the editor tells a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum ToPlugin {
    /// The plugin was loaded (or started again after it trapped): draw.
    Start,
    /// Something changed that the plugin may want to draw again.
    Refresh,
    #[serde(other)]
    Unknown,
}

/// What a plugin tells the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum FromPlugin {
    /// Replace what the plugin's pane shows.
    SetView { view: WireView },
    #[serde(other)]
    Unknown,
}

/// What a plugin's pane shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum WireView {
    Lines {
        #[serde(default)]
        lines: Vec<WireLine>,
    },
    List {
        #[serde(default)]
        items: Vec<String>,
        #[serde(default)]
        selected: Option<usize>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WireLine {
    #[serde(default)]
    pub text: String,
    /// The theme slot that styles it.
    #[serde(default)]
    pub slot: String,
}

/// The slot of the line that stands in for a view this version cannot draw.
const PLACEHOLDER_SLOT: &str = "explorer.hint";

impl WireView {
    /// What to draw. A kind of view this version does not know is drawn as a line
    /// that says so, not dropped, so the pane does not look broken for no reason.
    pub(crate) fn into_node(self) -> ViewNode {
        match self {
            Self::Lines { lines } => ViewNode::Lines(
                lines
                    .into_iter()
                    .map(|line| ViewLine {
                        text: line.text.into(),
                        slot: line.slot.into(),
                    })
                    .collect(),
            ),
            Self::List { items, selected } => ViewNode::List {
                items: items.into_iter().map(Into::into).collect(),
                selected,
            },
            Self::Unknown => ViewNode::Lines(vec![ViewLine {
                text: "(this plugin sent a view this version cannot show)".into(),
                slot: PLACEHOLDER_SLOT.into(),
            }]),
        }
    }
}
