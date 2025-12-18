use crate::{
    key_controller::{InputEvent, InsertKind, KeyDoneKind},
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
        _ => KeyDoneKind::None,
    };

    match done_event {
        KeyDoneKind::None => (),
        KeyDoneKind::ToMainWindow => return Ok(Some(SessionEvent::ToMainWindow)),
    }

    match event {
        InputEvent::Exit => Ok(Some(SessionEvent::Exit)),
        InputEvent::Back => Ok(Some(SessionEvent::Back)),
        InputEvent::Remove => Ok(Some(SessionEvent::OnRemove)),
        InputEvent::SaveFile => Ok(Some(SessionEvent::SaveFile)),
        InputEvent::OpenLookup => Ok(Some(SessionEvent::OpenLookup)),
        InputEvent::TestDebugEvent => Ok(Some(SessionEvent::TestDebugEvent)),
        InputEvent::OpenCommandPrompt => Ok(Some(SessionEvent::OpenCommandPrompt)),
        InputEvent::OpenFileTreeWindow => Ok(Some(SessionEvent::OpenFileTreeWindow)),
        InputEvent::Enter | InputEvent::Insert(_) => Ok(Some(SessionEvent::OnInsert)),

        InputEvent::None
        | InputEvent::Left
        | InputEvent::Right
        | InputEvent::Up
        | InputEvent::Down => Ok(None),
    }
}
