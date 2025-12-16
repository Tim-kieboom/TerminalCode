use ratatui::{Frame, widgets::Block};

use crate::{
    key_controller::key_controller::KeyController, session::FileContext, window::{lookup_bar::LookupBar, text_editor::TextEditor}
};

pub mod lookup_bar;
pub mod text_editor;

macro_rules! impl_window_for_enum {
    ($enum_name:ident { $($variant:ident),* $(,)? }) => {
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

            fn new_key_controller<'a>(&'a mut self, file_saved: &'a mut FileContext) -> KeyController<'a> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.new_key_controller(file_saved),
                    )*
                }
            }
        }
    };
}

pub trait Window {
    fn on_insert(&mut self, file_context: &FileContext);
    fn draw_ui(&mut self, frame: &mut Frame, header: Block);
    fn new_key_controller<'a>(&'a mut self, file_context: &'a mut FileContext) -> KeyController<'a>;
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
