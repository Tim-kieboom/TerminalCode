//! What a plugin may ask of the editor that shows something or starts something: a popup of
//! text, and a program under the debugger (plugin API 1.11).

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::App;
use super::debug::PLUGIN_CONFIG;
use super::run::tab_wake;
use super::state::Popup;
use crate::components::popups::plugin_readme::Readme;
use crate::launch::{ConfigId, RunConfig};
use crate::plugin_api::{Refusal, ToPlugin};
use crate::pty::Session;

impl App {
    /// A plugin has text to show. It is not shown over another popup (a plugin does not take
    /// the screen from what the user is doing).
    pub(super) fn plugin_text_popup(&mut self, plugin: &str, title: String, text: String) {
        if !matches!(self.state.popup(), Popup::None) {
            return;
        }
        let title = match title.trim() {
            "" => plugin.to_owned(),
            title => title.to_owned(),
        };
        self.state.open_plugin_readme(Readme::plain(title, text));
    }

    /// A plugin with the capability asks to debug a program in a folder that is trusted: the
    /// program is started under the debugger in the tab of the plugin, and the plugin is told
    /// whether the session started. What the session does is the Debug pane's, as for the
    /// project's own.
    pub(super) fn plugin_debug(
        &mut self,
        plugin: &str,
        request: u64,
        (program, args, env): (String, Vec<String>, Vec<String>),
        cwd: PathBuf,
    ) {
        if self.debug_active() {
            self.send_to_plugin(
                plugin,
                ToPlugin::Refused {
                    request,
                    reason: Refusal::Busy,
                },
            );
            return;
        }
        let Some(events) = self.background.sender() else {
            self.plugin_debug_failed(plugin, request, "the event loop is not running");
            return;
        };
        let env: BTreeMap<String, String> = env
            .iter()
            .filter_map(|pair| pair.split_once('='))
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect();
        let config = RunConfig::built_program(
            ConfigId::from(format!("{PLUGIN_CONFIG}{plugin}").as_str()),
            format!("{plugin}: debug"),
            program.clone(),
            args,
            env,
            Some(cwd.display().to_string()),
        );
        let (rows, columns) = self.terminal_dimensions();
        let (session, feeder) = Session::detached(rows, columns, tab_wake(&events));
        if !self.place_run_tab(&config, session) {
            self.plugin_debug_failed(
                plugin,
                request,
                "there is no terminal tab for it (the editor says why)",
            );
            return;
        }
        self.start_debug(&config, PathBuf::from(program), feeder);
        match self.debug_active() {
            true => self.send_to_plugin(plugin, ToPlugin::Done { request }),
            false => self.plugin_debug_failed(
                plugin,
                request,
                "the debugger did not start (its tab says why)",
            ),
        }
    }

    fn plugin_debug_failed(&self, plugin: &str, request: u64, detail: &str) {
        self.send_to_plugin(
            plugin,
            ToPlugin::Refused {
                request,
                reason: Refusal::Failed {
                    detail: detail.to_owned(),
                },
            },
        );
    }
}
