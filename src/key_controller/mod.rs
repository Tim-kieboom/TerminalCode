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
    Insert(char),
    Remove,

    SaveFile,

    OpenLookup,
}
