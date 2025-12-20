use anyhow::Result;
use crossterm::event;
use std::char::ToUppercase;

use crate::key_controller::key_controller::SessionEvent;

pub mod default_controls;
pub mod input_event;
pub mod key_controller;

pub(crate) enum WindowControlReponse {
    None,
    ToMainWindow,
}

pub(crate) trait WindowsControl {
    fn move_up(&mut self) -> Result<WindowControlReponse>;
    fn move_down(&mut self) -> Result<WindowControlReponse>;
    fn move_left(&mut self, amount: u16) -> Result<WindowControlReponse>;
    fn move_right(&mut self, amount: u16) -> Result<WindowControlReponse>;
    
    fn enter(&mut self) -> Result<WindowControlReponse>;
    fn backspace(&mut self) -> Result<WindowControlReponse>;
    fn insert(&mut self, insert: InsertKind) -> Result<WindowControlReponse>;

    fn custom_action(&mut self, event: event::Event) -> Result<Option<SessionEvent>>;
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
    OpenCommandPrompt,
    OpenFileTreeWindow,

    TestDebugEvent,
}

#[derive(Debug, Clone)]
pub enum InsertKind {
    Char(char),
    UpperCase(ToUppercase),
}
