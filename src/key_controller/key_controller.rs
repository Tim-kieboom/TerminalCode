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
    OpenCommandPrompt,
}

pub fn handle_input<T: Window>(
    this: &mut T,
    mut event: InputEvent,
) -> Result<Option<SessionEvent>> {
    let done_event = match &mut event {
        InputEvent::Left => this.move_left(1)?,
        InputEvent::Right => this.move_right(1)?,
        InputEvent::Up => this.move_up()?,
        InputEvent::Down => this.move_down()?,
        InputEvent::Enter => this.enter()?,
        InputEvent::Remove => this.backspace()?,
        InputEvent::Insert(insert) => {
            let mut take = InsertKind::Char(' ');
            std::mem::swap(&mut take, insert);
            this.insert(take)?
        }
        _ => KeyDoneKind::None,
    };

    match done_event {
        KeyDoneKind::None => (),
        KeyDoneKind::CloseWindow => return Ok(Some(SessionEvent::Back)),
    }

    match event {
        InputEvent::Enter | InputEvent::Insert(_) => Ok(Some(SessionEvent::OnInsert)),
        InputEvent::Remove => Ok(Some(SessionEvent::OnRemove)),

        InputEvent::Exit => Ok(Some(SessionEvent::Exit)),
        InputEvent::Back => Ok(Some(SessionEvent::Back)),
        InputEvent::SaveFile => Ok(Some(SessionEvent::SaveFile)),
        InputEvent::OpenLookup => Ok(Some(SessionEvent::OpenLookup)),
        InputEvent::OpenCommandPrompt => Ok(Some(SessionEvent::OpenCommandPrompt)),

        _ => Ok(None),
    }
}
