use crate::component::PluginViewId;
use crate::ui::view::ViewNode;

/// Non-input events delivered to the app loop (PTY output, file watcher,
/// parse results, plugin messages, ...). Terminal input has its own channel
/// so that it is never queued behind these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    SetPluginView { id: PluginViewId, content: ViewNode },
}
