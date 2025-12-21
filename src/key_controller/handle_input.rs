use std::path::PathBuf;

use crate::{
    key_controller::{InputEvent, InsertKind, WindowControlReponse},
    window::Window,
};
use anyhow::Result;

/// High-level events dispatched from input handling to session management.
///
/// Trigger window stack changes, file operations, and IDE navigation.
pub enum SessionEvent {
    /// Immediately exit the entire IDE session.
    Exit,
    /// Pop current window from stack (go back).
    Back,
    /// Save current file to disk.
    SaveFile,
    /// Text was inserted (trigger `Window::on_insert()` dirty marking).
    OnInsert,
    /// Text was removed (trigger `Window::on_remove()` dirty marking).
    OnRemove,
    /// Open fuzzy file finder.
    OpenLookup,
    /// Return to main `TextEditor` window and pop other windows.
    ToMainWindow,
    /// Open integrated shell/terminal.
    OpenCommandPrompt,
    /// Open hierarchical file browser.
    OpenFileTreeWindow,
    /// Open file creation dialog at specific path.
    OpenFileCreater { in_path: PathBuf },

    /// Debug error testing.
    TestDebugEvent,
}

/// Central input dispatcher for all windows.
///
/// 1. **Delegates** to `WindowsControl` trait methods (`move_up`, `enter`, etc.)
/// 2. **Maps** low-level `InputEvent` → high-level `SessionEvent`
/// 3. **Chains** window response + session event for complex flows
pub fn handle_input<T: Window>(
    this: &mut T,
    mut event: InputEvent,
) -> Result<Option<SessionEvent>> {
    let done_event = match &mut event {
        InputEvent::Up => this.move_up()?,
        InputEvent::Enter => this.enter()?,
        InputEvent::Down => this.move_down()?,
        InputEvent::Left => this.move_left(1)?,
        InputEvent::Remove => this.backspace()?,
        InputEvent::Right => this.move_right(1)?,
        InputEvent::Insert(insert) => {
            let mut take = InsertKind::Char(' ');
            std::mem::swap(&mut take, insert);
            this.insert(take)?
        }
        _ => WindowControlReponse::None,
    };

    match done_event {
        WindowControlReponse::None => (),
        WindowControlReponse::ToMainWindow => return Ok(Some(SessionEvent::ToMainWindow)),
    }

    let session_event = match event {
        InputEvent::Exit => SessionEvent::Exit,
        InputEvent::Back => SessionEvent::Back,
        InputEvent::Remove => SessionEvent::OnRemove,
        InputEvent::SaveFile => SessionEvent::SaveFile,
        InputEvent::OpenLookup => SessionEvent::OpenLookup,
        InputEvent::TestDebugEvent => SessionEvent::TestDebugEvent,
        InputEvent::OpenCommandPrompt => SessionEvent::OpenCommandPrompt,
        InputEvent::OpenFileTreeWindow => SessionEvent::OpenFileTreeWindow,
        InputEvent::Enter | InputEvent::Insert(_) => SessionEvent::OnInsert,

        InputEvent::None
        | InputEvent::Left
        | InputEvent::Right
        | InputEvent::Up
        | InputEvent::Down => return Ok(None),
    };

    Ok(Some(session_event))
}
