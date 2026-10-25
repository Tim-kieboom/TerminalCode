//! The watches: expressions the user pinned, asked for again at every stop in the frame chosen
//! (with the context `hover`, so nothing in the program can change), each a row with its value
//! that opens to its children. The rows that were open are opened again after a stop.

use std::time::Instant;

use super::evaluate::{incoming, words};
use super::{Active, Kind, Target, Waiting, incoming_children};
use crate::app::App;
use crate::components::debug::model::{Incoming, Read};
use crate::dap::{DapError, EvaluateArguments, EvaluateResponse};

/// What the adapter said, in one line (a row has one).
fn one_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_owned()
}

impl App {
    /// The program stopped, or another frame was chosen: every watch is asked for again, and the
    /// rows that were open are opened again as the answers come.
    pub(super) fn debug_watches_refresh(&mut self, active: &mut Active) {
        let data = self.state.debug_mut();
        if data.watches.rows().is_empty() {
            return;
        }
        data.watch_restore = data.watches.open_paths();
        data.watch_busy = 0;
        for expression in data.watches.roots() {
            self.debug_watch_evaluate(active, &expression);
        }
    }

    /// Asks for `expression` in the frame chosen.
    fn debug_watch_evaluate(&mut self, active: &mut Active, expression: &str) {
        let Some(frame) = self.state.debug().frame().map(|frame| frame.id) else {
            return;
        };
        let arguments = EvaluateArguments::hover(expression, frame);
        match active.client.evaluate(&arguments) {
            Ok(reply) => {
                self.state.debug_mut().watch_busy += 1;
                active.waiting.push(Waiting {
                    at: Instant::now(),
                    kind: Kind::Watch {
                        expression: expression.to_owned(),
                        reply,
                    },
                });
            }
            Err(error) => {
                let message = one_line(&error.to_string());
                self.debug_watch_comes_to(expression, error_row(message));
            }
        }
    }

    /// A watch pinned while the program is stopped is asked for at once.
    pub(in crate::app) fn debug_watch_ask_now(&mut self, expression: &str) {
        if !self.debug_can_evaluate() {
            return;
        }
        let Some(mut active) = self.debug.session.take() else {
            return;
        };
        self.debug_watch_evaluate(&mut active, expression);
        self.debug.session = Some(active);
    }

    pub(super) fn debug_watch_evaluated(
        &mut self,
        active: &mut Active,
        expression: &str,
        result: Result<EvaluateResponse, DapError>,
    ) {
        let data = self.state.debug_mut();
        data.watch_busy = data.watch_busy.saturating_sub(1);
        let came_to = match result {
            Ok(response) => incoming(response),
            Err(error) => error_row(one_line(&words(&error))),
        };
        self.debug_watch_comes_to(expression, came_to);
        self.debug_watch_reopen(active);
    }

    fn debug_watch_comes_to(&mut self, expression: &str, came_to: Incoming) {
        self.state
            .debug_mut()
            .watches
            .replace_root(expression, came_to);
        self.needs_redraw = true;
    }

    /// Opens the rows that were open before the stop and have appeared; once nothing is
    /// waited for, what is left of them is forgotten.
    fn debug_watch_reopen(&mut self, active: &mut Active) {
        let data = self.state.debug_mut();
        let reads = data.watches.reopen(&mut data.watch_restore);
        data.watch_busy += reads.len();
        if data.watch_busy == 0 {
            data.watch_restore.clear();
        }
        for read in reads {
            self.debug_ask_watch_variables(active, read);
        }
    }

    fn debug_ask_watch_variables(&mut self, active: &mut Active, read: Read) {
        if Self::debug_ask_children(active, &read, Target::Watch) {
            return;
        }
        let data = self.state.debug_mut();
        data.watches.failed(read.row);
        data.watch_busy = data.watch_busy.saturating_sub(1);
    }

    pub(super) fn debug_watch_variables(
        &mut self,
        active: &mut Active,
        row: u64,
        start: i64,
        result: Result<crate::dap::VariablesResponse, DapError>,
    ) {
        let data = self.state.debug_mut();
        data.watch_busy = data.watch_busy.saturating_sub(1);
        match result {
            Ok(response) => data.watches.fill(row, start, incoming_children(response)),
            Err(_) => data.watches.failed(row),
        }
        self.debug_watch_reopen(active);
        self.needs_redraw = true;
    }

    /// Opens or closes the row `index` of the watches.
    pub(in crate::app) fn debug_watch_toggle(&mut self, index: usize) {
        let Some(read) = self.state.debug_mut().watches.toggle(index) else {
            return;
        };
        self.state.debug_mut().watch_busy += 1;
        match self.debug.session.take() {
            Some(mut active) => {
                self.debug_ask_watch_variables(&mut active, read);
                self.debug.session = Some(active);
            }
            None => self.state.debug_mut().watches.failed(read.row),
        }
        self.needs_redraw = true;
    }
}

/// A row for what the adapter would not evaluate: its words in place of the value.
fn error_row(message: String) -> Incoming {
    Incoming {
        value: message,
        ..Incoming::default()
    }
}
