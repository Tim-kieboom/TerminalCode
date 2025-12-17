use anyhow::Result;
use ratatui::{Frame, widgets::Block};

use crate::{
    key_controller::{InsertKind, KeyController, KeyDoneKind},
    session::FileContext,
    window::{lookup_bar::LookupBar, text_editor::TextEditor},
};

pub mod lookup_bar;
pub mod text_editor;

macro_rules! impl_window_for_enum {
    ($enum_name:ident { $($variant:ident),* $(,)? }) => {
        impl KeyController for $enum_name {
            fn move_up(&mut self) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_up(),
                    )*
                }
            }
            fn move_down(&mut self) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_down(),
                    )*
                }
            }
            fn move_left(&mut self, amount: u16) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_left(amount),
                    )*
                }
            }
            fn move_right(&mut self, amount: u16) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_right(amount),
                    )*
                }
            }
            fn enter(&mut self) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.enter(),
                    )*
                }
            }
            fn backspace(&mut self) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.backspace(),
                    )*
                }
            }
            fn insert(&mut self, insert: InsertKind) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.insert(insert),
                    )*
                }
            }
        }

        impl Window for $enum_name {
            fn on_insert(&mut self, file_context: &FileContext) {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.on_insert(file_context),
                    )*
                }
            }

            fn draw_ui(&mut self, frame: &mut Frame, header: Block) {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.draw_ui(frame, header),
                    )*
                }
            }
        }
    };
}

pub trait Window: KeyController {
    fn on_insert(&mut self, file_context: &FileContext);
    fn draw_ui(&mut self, frame: &mut Frame, header: Block);
}

#[derive(Debug)]
pub enum WindowKind {
    TextEditor(TextEditor),
    LookupBar(LookupBar),
}
impl_window_for_enum!(WindowKind {
    TextEditor,
    LookupBar,
});
