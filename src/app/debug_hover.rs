//! The hover box of the debugger: its keys, its mouse, and taking it away. What it shows and
//! asks of the adapter is the session's (`debug/evaluate.rs`).

use std::ops::Range;

use crossterm::event::MouseEventKind;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent};
use ratatui::layout::Position;

use super::App;
use super::debug::evaluate::Ask;
use crate::event::action::Action;
use crate::keymap::KeyChord;

/// Rows one wheel notch scrolls.
const WHEEL_ROWS: isize = 3;
/// The longest expression that is sent to the adapter.
const LONGEST: usize = 300;

impl App {
    /// Takes the box and the expression it waits for away. Whether a box was there.
    pub(super) fn close_debug_hover(&mut self) -> bool {
        self.debug_hover_forget_pending();
        let had = self.state.debug_hover().is_some();
        if had {
            self.state.set_debug_hover(None);
            self.needs_redraw = true;
        }
        had
    }

    /// The bytes the primary selection covers in `document`, when it is the one in front, is not
    /// empty, is on one line, and holds `byte`.
    pub(super) fn selection_holding(
        &self,
        document: crate::ids::DocumentId,
        byte: usize,
    ) -> Option<Range<usize>> {
        if self.state.workspace().active_document_id() != Some(document) {
            return None;
        }
        let editor = self.state.editor();
        let selection = editor.selections().primary();
        if selection.is_empty() || selection.start().line != selection.end().line {
            return None;
        }
        let buffer = editor.buffer();
        let start = buffer.position_to_byte(selection.start()).ok()?;
        let end = buffer.position_to_byte(selection.end()).ok()?;
        (start..end).contains(&byte).then_some(start..end)
    }

    /// What the debugger is asked for the thing at `range` of `text`: the selection when it is
    /// one, else a word that is a name (not a number). `None` for what is not worth asking.
    pub(super) fn debug_expression(
        text: &ropey::Rope,
        range: &Range<usize>,
        selection: bool,
    ) -> Option<String> {
        let slice = text.get_byte_slice(range.clone())?.to_string();
        let expression = slice.trim();
        let name = expression
            .chars()
            .next()
            .is_some_and(|c| !c.is_ascii_digit());
        let fit = !expression.is_empty() && expression.len() <= LONGEST;
        (fit && (selection || name)).then(|| expression.to_owned())
    }

    /// `f6` while the program is stopped: asks for the selection, or the word, at the cursor, for
    /// a box that stays. Whether it did.
    pub(super) fn ask_debug_hover_at_cursor(&mut self) -> bool {
        if !self.debug_can_evaluate() {
            return false;
        }
        let Some(document) = self.state.workspace().active_document_id() else {
            return false;
        };
        let editor = self.state.editor();
        let head = editor.selections().primary().head();
        let Ok(cursor) = editor.buffer().position_to_byte(head) else {
            return false;
        };
        let text = editor.buffer().snapshot();
        let selected = {
            let selection = editor.selections().primary();
            (!selection.is_empty() && selection.start().line == selection.end().line)
                .then(|| {
                    let start = editor.buffer().position_to_byte(selection.start()).ok()?;
                    let end = editor.buffer().position_to_byte(selection.end()).ok()?;
                    Some(start..end)
                })
                .flatten()
        };
        let (range, selection) = match selected {
            Some(range) => (range, true),
            None => {
                let at = [Some(cursor), cursor.checked_sub(1)]
                    .into_iter()
                    .flatten()
                    .find_map(|byte| super::hover::word_around(&text, byte));
                match at {
                    Some(word) => (word, false),
                    None => return false,
                }
            }
        };
        let Some(expression) = Self::debug_expression(&text, &range, selection) else {
            return false;
        };
        let screen = self.state.screen();
        let anchor = self
            .state
            .editor()
            .cursor_cell()
            .unwrap_or_else(|| Position::new(screen.width / 4, screen.height / 3));
        self.close_hover();
        self.debug_hover_ask(Ask {
            expression,
            selection,
            document,
            byte: cursor,
            anchor,
            word: None,
            pinned: true,
        })
    }

    /// `f6` with a box of the debugger up: keeps it; on one that is kept, takes it away.
    pub(super) fn pin_debug_hover(&mut self) -> bool {
        let Some(shown) = self.state.debug_hover_mut() else {
            return false;
        };
        if shown.is_pinned() {
            self.close_debug_hover();
        } else {
            shown.pin();
            self.needs_redraw = true;
        }
        true
    }

    /// A key while the box of the debugger is up. One that was kept (a click in it, or `f6`) has
    /// the arrow and page keys, `right` and `left` to open and close a row, and `enter`; `esc`
    /// takes either away and does nothing else; any other key takes it away and does its work.
    /// Whether the key was used up.
    pub(super) fn handle_debug_hover_key(&mut self, key: KeyEvent) -> bool {
        let Some(shown) = self.state.debug_hover() else {
            return false;
        };
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let for_the_box = KeyChord::from_event(key).is_some_and(|chord| {
            self.keyboard.action_for(self.contexts(), chord) == Some(&Action::ShowHover)
        });
        // Going on or stepping does not take the box away: it shows the program as it stops.
        let controls = KeyChord::from_event(key).is_some_and(|chord| {
            matches!(
                self.keyboard.action_for(self.contexts(), chord),
                Some(
                    Action::Debug
                        | Action::DebugContinue
                        | Action::DebugStepOver
                        | Action::DebugStepInto
                        | Action::DebugStepOut
                        | Action::DebugStop
                        | Action::DebugRestart
                )
            )
        });
        if for_the_box || controls {
            return false;
        }
        if shown.is_pinned() && key.modifiers == KeyModifiers::NONE {
            let page = shown.page();
            let last = isize::try_from(shown.tree().rows().len()).unwrap_or(isize::MAX);
            let selected = shown.selected();
            let open = shown.tree().rows().get(selected).map(|row| row.expanded);
            let used = match key.code {
                KeyCode::Up => Some(-1),
                KeyCode::Down => Some(1),
                KeyCode::PageUp => Some(-page),
                KeyCode::PageDown => Some(page),
                KeyCode::Home => Some(-last),
                KeyCode::End => Some(last),
                KeyCode::Right | KeyCode::Left | KeyCode::Enter => Some(0),
                _ => None,
            };
            if let Some(by) = used {
                let toggles = matches!(
                    (key.code, open),
                    (KeyCode::Enter, Some(_))
                        | (KeyCode::Right, Some(false))
                        | (KeyCode::Left, Some(true))
                );
                if let Some(shown) = self.state.debug_hover_mut() {
                    shown.move_by(by);
                }
                if toggles {
                    self.debug_hover_toggle(selected);
                }
                self.needs_redraw = true;
                return true;
            }
        }
        self.close_debug_hover();
        key.code == KeyCode::Esc
    }

    /// The mouse on the box: a click keeps it and opens or closes the row under it, the wheel
    /// scrolls it. Whether the event was used up.
    pub(super) fn handle_debug_hover_mouse(&mut self, event: MouseEvent) -> bool {
        let Some(shown) = self.state.debug_hover() else {
            return false;
        };
        if !shown.contains(event.column, event.row) {
            return false;
        }
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let row = shown.row_at(event.column, event.row);
                if let Some(shown) = self.state.debug_hover_mut() {
                    shown.pin();
                    if let Some(row) = row {
                        shown.select(row);
                    }
                }
                if let Some(row) = row {
                    self.debug_hover_toggle(row);
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let rows = match event.kind == MouseEventKind::ScrollDown {
                    true => WHEEL_ROWS,
                    false => -WHEEL_ROWS,
                };
                if let Some(shown) = self.state.debug_hover_mut() {
                    shown.scroll_by(rows);
                }
            }
            _ => return false,
        }
        self.needs_redraw = true;
        true
    }
}
