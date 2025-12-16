use std::char::ToUppercase;

pub mod input_event;
pub mod key_controller;

pub enum InputEvent {
    None,

    Left,
    Right,
    Up,
    Down,

    Exit,
    Back,

    Enter,
    Insert(InsertKind),
    Remove,

    SaveFile,

    OpenLookup,
}

pub enum InsertKind {
    Char(char),
    String(String),
    UpperCase(ToUppercase),
}
