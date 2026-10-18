//! Plugins: the thread they run on, and what they tell the app.

use super::App;
use crate::event::Event;
use crate::plugins::{Notice, Runtime};
use crate::ui::PluginViewId;

/// The plugin thread, once the app can hear from it.
#[derive(Debug, Default)]
pub(super) struct Plugins {
    runtime: Option<Runtime>,
}

impl App {
    /// Starts the thread plugins run on; what it says comes back as events.
    pub(super) fn start_plugin_runtime(&mut self) {
        let Some(events) = self.background.sender() else {
            return;
        };
        self.plugins.runtime = Some(Runtime::spawn(move |notice| {
            let _ = events.blocking_send(Event::Plugin(notice));
        }));
    }

    /// What the plugin thread said: a new view, or what happened to a plugin.
    pub(super) fn plugin_notice(&mut self, notice: Notice) {
        match notice {
            Notice::View { plugin, content } => {
                self.state
                    .set_plugin_view(PluginViewId::new(&*plugin), content);
            }
            Notice::Restarted { plugin, reason } => self
                .state
                .notify(format!("plugin {plugin} restarted: {reason}")),
            Notice::Disabled { plugin, reason } => self.state.notify_error(format!(
                "plugin {plugin} is off: it failed three times within a minute (last: {reason})"
            )),
            Notice::Broken { plugin, reason } => self
                .state
                .notify_error(format!("plugin {plugin} cannot run: {reason}")),
            Notice::Rejected { plugin, reason } => self
                .state
                .notify_error(format!("plugin {plugin} said something wrong: {reason}")),
        }
        self.needs_redraw = true;
    }

    /// Loads a plugin from its module and starts it.
    #[cfg(test)]
    pub(crate) fn load_plugin(&self, plugin: &str, wasm: Vec<u8>) {
        if let Some(runtime) = &self.plugins.runtime {
            runtime.load(plugin, wasm);
        }
    }

    /// Tells a plugin that something changed.
    #[cfg(test)]
    pub(crate) fn refresh_plugin(&self, plugin: &str) {
        if let Some(runtime) = &self.plugins.runtime {
            runtime.refresh(plugin);
        }
    }
}
