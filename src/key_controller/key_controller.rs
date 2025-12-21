use std::path::PathBuf;

use crate::{
    key_controller::{InputEvent, InsertKind, WindowControlReponse},
    window::Window,
};
use anyhow::Result;

pub enum SessionEvent {
    Exit,
    Back,
    SaveFile,
    OnInsert,
    OnRemove,
    OpenLookup,
    ToMainWindow,
    OpenCommandPrompt,
    OpenFileTreeWindow,
    TestDebugEvent,
    OpenFileCreater{in_path: PathBuf},
}

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
