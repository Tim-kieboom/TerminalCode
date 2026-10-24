//! The watches: pinning a variable of what is on the screen (a row of the variables, a row of
//! the hover box), taking one away, and remembering them for the project. What they come to is
//! the session's (`debug/watch.rs`).

use super::App;
use super::plugin_trust::trust_key;
use crate::components::debug::Section;

/// The expression of a row of a tree from the names above it: `a`, `x` and `[0]` make `a.x[0]`.
pub(crate) fn path_expression(path: &[String]) -> String {
    let mut expression = String::new();
    for (index, name) in path.iter().enumerate() {
        if index > 0 && !name.starts_with('[') {
            expression.push('.');
        }
        expression.push_str(name);
    }
    expression
}

impl App {
    /// Takes in the watches remembered for this project.
    pub(super) fn import_saved_watches(&mut self) {
        let project = trust_key(&self.project_root());
        for expression in self.saved_watches(&project) {
            self.state.debug_mut().add_watch(&expression);
        }
    }

    /// Writes the watches to `state.json` when they are not what is there.
    fn remember_watches(&mut self) {
        let project = trust_key(&self.project_root());
        let watches = self.state.debug().watches.roots();
        self.store_watches(&project, watches);
    }

    /// Adds the watch `expression` (once) and, while the program is stopped, asks for its value.
    pub(super) fn pin_watch(&mut self, expression: &str) {
        if expression.is_empty() {
            return;
        }
        if !self.state.debug_mut().add_watch(expression) {
            self.state
                .notify(format!("{expression} is watched already"));
            return;
        }
        self.remember_watches();
        self.debug_watch_ask_now(expression);
        self.state.show_debug();
        self.needs_redraw = true;
    }

    /// `w` in the pane: the variable the selection is on becomes a watch.
    pub(super) fn pin_selected_variable(&mut self) {
        if self.state.debug_view().section() != Section::Variables {
            self.state
                .notify("w pins the variable that is selected in the variables");
            return;
        }
        let index = self.state.debug_view().selected_variable();
        let path = self.state.debug().tree.path_of(index);
        // The first name is the scope (`Locals`): a watch starts at the variable.
        match path.split_first() {
            Some((_, variable)) if !variable.is_empty() => {
                let expression = path_expression(variable);
                self.pin_watch(&expression);
            }
            _ => self
                .state
                .notify("a scope cannot be watched: pin a variable of it"),
        }
    }

    /// `delete` in the pane: the watch the selection is on is taken away.
    pub(super) fn remove_selected_watch(&mut self) {
        if self.state.debug_view().section() != Section::Watches {
            return;
        }
        let index = self.state.debug_view().selected_watch();
        let before = self.state.debug().watches.rows().len();
        self.state.debug_mut().watches.remove_root(index);
        if self.state.debug().watches.rows().len() != before {
            self.remember_watches();
            self.needs_redraw = true;
        }
    }
}
