use serde::Deserialize;

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

/// Every kind of pane that can appear in a layout.
///
/// The enum is closed except for [`ComponentKind::Plugin`], the single hole
/// through which plugins add their own panes. Component state lives in
/// `AppState`, never in the component itself.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) enum ComponentKind {
    Explorer,
    Editor,
    Terminal,
    StatusBar,
    Plugin(PluginViewId),
}

impl ComponentKind {
    pub(crate) fn title(&self) -> &str {
        match self {
            Self::Explorer => "Explorer",
            Self::Editor => "Editor",
            Self::Terminal => "Terminal",
            Self::StatusBar => "Status",
            Self::Plugin(id) => id.as_str(),
        }
    }
}
