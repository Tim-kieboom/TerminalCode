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
            Char(char) => handle_char(char, key.modifiers),

            Esc => {
                if key.modifiers == KeyModifiers::SHIFT {
                    return InputEvent::Exit;
                } else {
                    return InputEvent::Back;
                }
            }
            _ => InputEvent::None,
        };
    }

    InputEvent::None
}

fn handle_char(char: char, modifiers: KeyModifiers) -> InputEvent {
    if modifiers.is_empty() {
        return InputEvent::Insert(InsertKind::Char(char));
    }

    if modifiers == KeyModifiers::SHIFT {
        return InputEvent::Insert(InsertKind::UpperCase(char.to_uppercase()));
    }

    key_combine(char, modifiers)
}

fn key_combine(char: char, modifiers: KeyModifiers) -> InputEvent {
    if modifiers == KeyModifiers::CONTROL {
        match char {
            'p' => InputEvent::OpenLookup,
            's' => InputEvent::SaveFile,
            _ => InputEvent::None,
        }
    } else {
        InputEvent::None
    }
}
