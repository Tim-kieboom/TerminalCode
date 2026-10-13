//! Where you have been: the jump list ([`JumpList`]) and what the app does
//! with it. A place is recorded when something takes the cursor far away (a
//! search or finder hit, go to line, a diagnostic, another file in front, a
//! mouse click more than a screen away), and `alt+,` and `alt+.` go back and
//! forward. Plain keyboard movement, typing and paging record nothing.

use crossterm::event::{Event as InputEvent, MouseButton, MouseEventKind};

use super::App;
use crate::jumplist::{Change, JumpList, Place, Step};

/// What the app keeps of the jump list.
#[derive(Debug, Default)]
pub(super) struct Jumps {
    list: JumpList,
    /// Something in the input being handled took the cursor to another place.
    marked: bool,
    /// The input being handled is a jump through the list, which records nothing.
    navigating: bool,
}

impl App {
    /// The place of the cursor in the file in front, if there is one.
    pub(super) fn here(&self) -> Option<Place> {
        if !self.state.workspace().has_tabs() {
            return None;
        }
        let document = self.state.workspace().active_document_id()?;
        let editor = self.state.editor();
        let head = editor.selections().primary().head();
        let byte = editor.buffer().position_to_byte(head).ok()?;
        Some(Place {
            document,
            path: editor.buffer().path().map(std::path::Path::to_path_buf),
            byte,
        })
    }

    /// Records `place`, which something took the cursor away from outside of an input
    /// (the answer of a server).
    pub(super) fn record_place(&mut self, place: Place) {
        self.jumps.list.record(place);
    }

    /// Something is about to take the cursor to another place on purpose; the
    /// place it leaves is recorded when the input has been handled.
    pub(super) fn mark_jump(&mut self) {
        self.jumps.marked = true;
    }

    /// What the app does around each input: the place the cursor was at, from
    /// before it, is recorded when the input was a jump.
    pub(super) fn track_jump(&mut self, input: &InputEvent, before: Option<Place>) {
        let events = self.state.workspace_mut().take_edit_events();
        let mut before = before;
        for (document, event) in events {
            let change = Change {
                start: event.start,
                old_end: event.old_end,
                new_end: event.new_end,
            };
            self.jumps.list.remap(document, change);
            before = before.map(|place| place.moved(document, change));
        }
        let marked = std::mem::take(&mut self.jumps.marked);
        if std::mem::take(&mut self.jumps.navigating) {
            return;
        }
        let (Some(before), Some(after)) = (before, self.here()) else {
            return;
        };
        // Another file in front, when the one that was is still open (closing a
        // tab is not a jump out of it).
        let switched = after.document != before.document
            && self.state.workspace().has_document(before.document);
        if marked || switched || self.far_click(input, &before, &after) {
            self.jumps.list.record(before);
        }
    }

    /// A press of the left button that took the cursor more than a screen away.
    fn far_click(&self, input: &InputEvent, before: &Place, after: &Place) -> bool {
        let InputEvent::Mouse(event) = input else {
            return false;
        };
        if event.kind != MouseEventKind::Down(MouseButton::Left)
            || before.document != after.document
        {
            return false;
        }
        let buffer = self.state.editor().buffer();
        let (Ok(from), Ok(to)) = (
            buffer.byte_to_position(before.byte),
            buffer.byte_to_position(after.byte),
        ) else {
            return false;
        };
        from.line.abs_diff(to.line) > usize::from(self.state.screen().height)
    }

    /// The place can be gone to: its document is open, or its file is there to open.
    fn place_usable(&self, place: &Place) -> bool {
        self.state.workspace().has_document(place.document)
            || place.path.as_ref().is_some_and(|path| path.exists())
    }

    /// `alt+,`: back to where the last jump left.
    pub(super) fn jump_back(&mut self) {
        let Some(here) = self.here() else {
            return;
        };
        let mut list = std::mem::take(&mut self.jumps.list);
        let step = list.back(here, |place| self.place_usable(place));
        self.jumps.list = list;
        self.go_through_list(step, "no earlier place");
    }

    /// `alt+.`: forward again after going back.
    pub(super) fn jump_forward(&mut self) {
        let mut list = std::mem::take(&mut self.jumps.list);
        let step = list.forward(|place| self.place_usable(place));
        self.jumps.list = list;
        self.go_through_list(step, "no later place");
    }

    fn go_through_list(&mut self, step: Option<Step>, nothing: &str) {
        let Some(step) = step else {
            self.state.notify(nothing);
            return;
        };
        self.jumps.navigating = true;
        if let Err(error) = self.go_to_place_of(&step.place) {
            self.state.notify_error(error);
            return;
        }
        if step.skipped > 0 {
            self.state.notify(match step.skipped {
                1 => "skipped one place whose file is gone".to_owned(),
                count => format!("skipped {count} places whose files are gone"),
            });
        }
        self.state.set_focus(super::state::Focus::Editor);
        self.needs_redraw = true;
    }

    /// Puts the cursor on `place`: in its document when that is open, opening
    /// its file again when the tab was closed.
    fn go_to_place_of(&mut self, place: &Place) -> Result<(), String> {
        if self.state.workspace().has_document(place.document) {
            self.state.workspace_mut().show_document(place.document);
        } else if let Some(path) = &place.path {
            self.state
                .workspace_mut()
                .open_path(path)
                .map_err(|error| error.to_string())?;
            if let Some(document) = self.state.workspace().active_document_id() {
                self.jumps.list.reopened(path, document);
            }
        } else {
            return Err("that place is in a file that was closed".to_owned());
        }
        let buffer = self.state.editor().buffer();
        let position = buffer
            .byte_to_position(place.byte.min(buffer.snapshot().len_bytes()))
            .map_err(|error| error.to_string())?;
        self.state.edit(|editor| editor.go_to(position));
        Ok(())
    }
}
