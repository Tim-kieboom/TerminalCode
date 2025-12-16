use crate::key_controller::{InputEvent, InsertKind};
use crossterm::event::{Event, KeyEventKind, KeyModifiers};

pub fn get_input_event(event: Event) -> InputEvent {
    use crossterm::event::KeyCode::*;

    if let Event::Key(key) = event {
        if key.kind != KeyEventKind::Press {
            return InputEvent::None;
        }

        return match key.code {
            Up => InputEvent::Up,
            Down => InputEvent::Down,
            Left => InputEvent::Left,
            Right => InputEvent::Right,
            Enter => InputEvent::Enter,
            Backspace => InputEvent::Remove,
            Char(c) if key.modifiers.is_empty() => InputEvent::Insert(InsertKind::Char(c)),
            Char(c) if key.modifiers == KeyModifiers::SHIFT => InputEvent::Insert(InsertKind::UpperCase(c.to_uppercase())),

            Esc => return InputEvent::Exit,
            Char(c) => key_combine(c, key.modifiers),
            _ => InputEvent::None,
        };
    }

    InputEvent::None
}

fn key_combine(char: char, modifier: KeyModifiers) -> InputEvent {
    if modifier == KeyModifiers::CONTROL {
        match char {
            'p' => InputEvent::OpenLookup,
            's' => InputEvent::SaveFile,
            _ => InputEvent::None,
        }
    } else {
        InputEvent::None
    }
}
