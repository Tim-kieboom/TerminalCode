use anyhow::Result;
use std::char::ToUppercase;

pub mod default_controls;
pub mod input_event;
pub mod key_controller;

pub(crate) enum KeyDoneKind {
    None,
    CloseWindow,
}

pub(crate) trait KeyController {
    fn move_up(&mut self) -> Result<KeyDoneKind>;
    fn move_down(&mut self) -> Result<KeyDoneKind>;
    fn move_left(&mut self, amount: u16) -> Result<KeyDoneKind>;
    fn move_right(&mut self, amount: u16) -> Result<KeyDoneKind>;
    fn enter(&mut self) -> Result<KeyDoneKind>;
    fn backspace(&mut self) -> Result<KeyDoneKind>;
    fn insert(&mut self, insert: InsertKind) -> Result<KeyDoneKind>;
}

#[derive(Debug, Clone)]
pub enum InputEvent {
    None,

    Left,
    Right,
    Up,
    Down,

    Exit,
    Back,

    Enter,
    Remove,
    Insert(InsertKind),

    SaveFile,

    OpenLookup,
}

#[derive(Debug, Clone)]
pub enum InsertKind {
    Char(char),
    UpperCase(ToUppercase),
}
