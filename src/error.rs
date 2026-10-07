use thiserror::Error;

use crate::buffer::BufferError;
use crate::ui::layout::LayoutError;
use crate::ui::theme::ThemeError;

pub type IdeResult<T = ()> = Result<T, IdeError>;

#[derive(Debug, Error)]
pub enum IdeError {
    #[error("{0}")]
    Unknown(Box<str>),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid layout: {0}")]
    Layout(#[from] LayoutError),
    #[error("invalid theme: {0}")]
    Theme(#[from] ThemeError),
    #[error("buffer error: {0}")]
    Buffer(#[from] BufferError),
}

impl From<std::convert::Infallible> for IdeError {
    fn from(never: std::convert::Infallible) -> Self {
        match never {}
    }
}
