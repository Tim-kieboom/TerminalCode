pub mod completion_box;
pub mod editor;
pub mod errors;
pub mod explorer;
pub mod hover_box;
pub mod menu_bar;
pub mod notifications;
pub mod popups;
mod render;
pub mod scroll_text;
pub mod status_view;
pub mod terminal;
pub mod workspace;
pub use render::prepare_and_render;
use serde::Deserialize;

use crate::ui::theme::BackgroundPane;

/// Every kind of pane that can appear in a layout.
///
/// The enum is closed except for [`ComponentKind::Plugin`], the single hole
/// through which plugins add their own panes. Component state lives in
/// `AppState`, never in the component itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub(crate) enum ComponentKind {
    Explorer,
    Editor,
    Terminal,
    StatusBar,
    /// The row at the top with the icon that opens the main menu.
    MenuBar,
    /// What the language servers say is wrong with the open documents.
    Errors,
    Plugin(PluginViewId),
}
impl ComponentKind {
    /// The `[background.<pane>]` table of the theme that colors this component.
    pub(crate) fn background_pane(&self) -> Option<BackgroundPane> {
        match self {
            Self::Explorer => Some(BackgroundPane::Explorer),
            Self::Editor => Some(BackgroundPane::Editor),
            Self::Terminal => Some(BackgroundPane::Terminal),
            Self::StatusBar => Some(BackgroundPane::StatusBar),
            Self::MenuBar => Some(BackgroundPane::StatusBar),
            Self::Errors => Some(BackgroundPane::Terminal),
            Self::Plugin(_) => None,
        }
    }

    pub(crate) fn title(&self) -> &str {
        match self {
            Self::Explorer => "Explorer",
            Self::Editor => "Editor",
            Self::Terminal => "Terminal",
            Self::StatusBar => "Status",
            Self::MenuBar => "Menu",
            Self::Errors => "Errors",
            Self::Plugin(id) => id.as_str(),
        }
    }
}

/// Identifies a view registered by a plugin. Layout files reference it by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub(crate) struct PluginViewId(Box<str>);
impl PluginViewId {
    pub(crate) fn new(id: impl Into<Box<str>>) -> Self {
        Self(id.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
