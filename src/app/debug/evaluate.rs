//! What the debugger says about an expression: the hover box. The pointer (or `f6`) asks for an
//! expression in the frame chosen, the adapter evaluates it with the context `hover` (variable
//! paths only: no calls and no operators, so nothing in the program changes), and the answer is
//! a tree in a box. When the program stops again the expression is asked for again and the rows
//! that were open are opened again, so the box is never older than the stop.

use std::ops::Range;

use super::{Active, Kind, Target, Waiting};
use crate::app::App;
use crate::app::state::Popup;
use crate::components::debug::DebugHoverBox;
use crate::components::debug::model::{Incoming, Read, Status};
use crate::dap::{DapError, EvaluateArguments, EvaluateResponse, VariablesArguments};
use crate::ids::DocumentId;
use ratatui::layout::Position;
use std::time::Instant;

/// What is asked about, and where the box goes.
pub(in crate::app) struct Ask {
    pub(in crate::app) expression: String,
    /// The expression is a selection: what the adapter says against it is shown. Of a word it is
    /// not (the language server is asked instead).
    pub(in crate::app) selection: bool,
    pub(in crate::app) document: DocumentId,
    /// The byte the pointer is on, which the language server is asked about when this is not
    /// answered.
    pub(in crate::app) byte: usize,
    pub(in crate::app) anchor: Position,
    /// The word the pointer rests on, while it is the pointer that asks.
    pub(in crate::app) word: Option<Range<usize>>,
    pub(in crate::app) pinned: bool,
}

struct Pending {
    ticket: u64,
    ask: Ask,
    /// The version of the text the expression is from.
    version: u64,
    /// Asked again after a stop: the paths of the rows that were open.
    refresh: Option<Vec<Vec<String>>>,
}

#[derive(Default)]
pub(super) struct Evaluating {
    last: u64,
    pending: Option<Pending>,
}

pub(super) fn incoming(response: EvaluateResponse) -> Incoming {
    Incoming {
        name: String::new(),
        value: response.result,
        type_name: response.type_name,
        reference: response.variables_reference,
        total: response.indexed_variables.or(response.named_variables),
    }
}

/// What the adapter said, as it said it.
pub(super) fn words(error: &DapError) -> String {
    match error {
        DapError::Rejected { rejection, .. } => rejection.describe(),
        other => other.to_string(),
    }
}

impl App {
    /// Whether the program is stopped in a frame whose expressions can be asked for.
    pub(in crate::app) fn debug_can_evaluate(&self) -> bool {
        self.debug.session.is_some()
            && matches!(self.state.debug().status, Status::Stopped { .. })
            && self.state.debug().frame().is_some()
    }

    /// Asks the adapter about `ask.expression`. Whether it did (when it did not, there is no
    /// session to ask).
    pub(in crate::app) fn debug_hover_ask(&mut self, ask: Ask) -> bool {
        if !self.debug_can_evaluate() {
            return false;
        }
        let Some(version) = self.state.workspace().document_version(ask.document) else {
            return false;
        };
        let Some(mut active) = self.debug.session.take() else {
            return false;
        };
        let asked = self.debug_evaluate(&mut active, ask, version, None);
        self.debug.session = Some(active);
        asked
    }

    fn debug_evaluate(
        &mut self,
        active: &mut Active,
        ask: Ask,
        version: u64,
        refresh: Option<Vec<Vec<String>>>,
    ) -> bool {
        let Some(frame) = self.state.debug().frame().map(|frame| frame.id) else {
            return false;
        };
        let arguments = EvaluateArguments::hover(ask.expression.clone(), frame);
        let Ok(reply) = active.client.evaluate(&arguments) else {
            return false;
        };
        let evaluating = &mut self.debug.evaluating;
        evaluating.last += 1;
        let ticket = evaluating.last;
        evaluating.pending = Some(Pending {
            ticket,
            ask,
            version,
            refresh,
        });
        active.waiting.push(Waiting {
            at: Instant::now(),
            kind: Kind::Evaluate { ticket, reply },
        });
        true
    }

    /// The program stopped, or another frame was chosen: the expression of the box that is up
    /// is asked for again, and the rows that were open are opened again when it comes.
    pub(super) fn debug_hover_refresh(&mut self, active: &mut Active) {
        let Some(shown) = self.state.debug_hover_mut() else {
            return;
        };
        let (expression, document, version, pinned) = (
            shown.expression().to_owned(),
            shown.document(),
            shown.version(),
            shown.is_pinned(),
        );
        let paths = shown.begin_refresh();
        let ask = Ask {
            expression,
            selection: true,
            document,
            byte: 0,
            anchor: Position::default(),
            word: None,
            pinned,
        };
        if !self.debug_evaluate(active, ask, version, Some(paths)) {
            self.close_debug_hover();
        }
    }

    pub(super) fn debug_evaluated(
        &mut self,
        active: &mut Active,
        ticket: u64,
        result: Result<EvaluateResponse, DapError>,
    ) {
        let pending = &mut self.debug.evaluating.pending;
        if pending.as_ref().map(|pending| pending.ticket) != Some(ticket) {
            return;
        }
        let Some(Pending {
            ask,
            version,
            refresh,
            ..
        }) = pending.take()
        else {
            return;
        };
        self.needs_redraw = true;
        if let Some(paths) = refresh {
            self.debug_hover_refreshed(active, result, paths);
            return;
        }
        match result {
            Ok(response) => {
                if self.debug_hover_still_wanted(&ask, version) {
                    let shown = DebugHoverBox::valued(
                        &ask.expression,
                        incoming(response),
                        ask.anchor,
                        ask.document,
                        version,
                        ask.pinned,
                    );
                    self.show_debug_hover(shown);
                }
            }
            // What was selected is answered in the adapter's words.
            Err(error) if ask.selection => {
                if self.debug_hover_still_wanted(&ask, version) {
                    let shown = DebugHoverBox::refused(
                        &ask.expression,
                        words(&error),
                        ask.anchor,
                        ask.document,
                        version,
                        ask.pinned,
                    );
                    self.show_debug_hover(shown);
                }
            }
            // A word the adapter cannot say anything about: the language server may.
            Err(_) => {
                if self.debug_hover_still_wanted(&ask, version) {
                    self.ask_hover_of_server(ask.document, ask.byte, ask.anchor, ask.word, false);
                }
            }
        }
    }

    /// The box of the stop before: the expression came to something again, or would not.
    fn debug_hover_refreshed(
        &mut self,
        active: &mut Active,
        result: Result<EvaluateResponse, DapError>,
        paths: Vec<Vec<String>>,
    ) {
        let Some(shown) = self.state.debug_hover_mut() else {
            return;
        };
        let reads = match result {
            Ok(response) => shown.refreshed(incoming(response), paths),
            Err(error) => {
                shown.refused_again(words(&error));
                Vec::new()
            }
        };
        for read in reads {
            self.debug_ask_hover_variables(active, read);
        }
    }

    /// Whether the answer is still wanted: the text is as it was, nothing covers the editor, and
    /// the pointer is still on the word it was on.
    fn debug_hover_still_wanted(&self, ask: &Ask, version: u64) -> bool {
        self.state.workspace().document_version(ask.document) == Some(version)
            && matches!(self.state.popup(), Popup::None)
            && ask
                .word
                .as_ref()
                .is_none_or(|word| self.hover_candidate_is(ask.document, word))
    }

    fn show_debug_hover(&mut self, shown: DebugHoverBox) {
        self.state.set_hover_box(None);
        self.state.set_debug_hover(Some(shown));
        self.needs_redraw = true;
    }

    // --- the children of the rows of the box ---

    fn debug_ask_hover_variables(&mut self, active: &mut Active, read: Read) {
        let arguments = match read.page {
            Some((start, count)) => VariablesArguments::page(read.reference, start, count),
            None => VariablesArguments::of(read.reference),
        };
        match active.client.variables(&arguments) {
            Ok(reply) => active.waiting.push(Waiting {
                at: Instant::now(),
                kind: Kind::Variables {
                    row: read.row,
                    start: read.page.map_or(0, |(start, _)| start),
                    target: Target::Hover,
                    reply,
                },
            }),
            Err(_) => {
                if let Some(shown) = self.state.debug_hover_mut() {
                    shown.tree_mut().failed(read.row);
                }
            }
        }
    }

    pub(super) fn debug_hover_variables(
        &mut self,
        active: &mut Active,
        row: u64,
        start: i64,
        result: Result<crate::dap::VariablesResponse, DapError>,
    ) {
        let Some(shown) = self.state.debug_hover_mut() else {
            return;
        };
        match result {
            Ok(response) => {
                let children = response
                    .variables
                    .into_iter()
                    .map(|variable| Incoming {
                        name: variable.name,
                        value: variable.value,
                        type_name: variable.type_name,
                        reference: variable.variables_reference,
                        total: variable.indexed_variables.or(variable.named_variables),
                    })
                    .collect();
                shown.tree_mut().fill(row, start, children);
            }
            Err(_) => shown.tree_mut().failed(row),
        }
        // The rows that were open before a stop open as they appear.
        let reads = shown.read_done();
        for read in reads {
            self.debug_ask_hover_variables(active, read);
        }
        self.needs_redraw = true;
    }

    /// Opens or closes the row `index` of the box.
    pub(in crate::app) fn debug_hover_toggle(&mut self, index: usize) {
        let Some(shown) = self.state.debug_hover_mut() else {
            return;
        };
        let Some(read) = shown.tree_mut().toggle(index) else {
            return;
        };
        shown.read_started();
        match self.debug.session.take() {
            Some(mut active) => {
                self.debug_ask_hover_variables(&mut active, read);
                self.debug.session = Some(active);
            }
            None => {
                if let Some(shown) = self.state.debug_hover_mut() {
                    shown.tree_mut().failed(read.row);
                }
            }
        }
        self.needs_redraw = true;
    }

    /// Forgets the expression that is asked for.
    pub(in crate::app) fn debug_hover_forget_pending(&mut self) {
        self.debug.evaluating.pending = None;
    }
}
