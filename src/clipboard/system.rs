use std::io::{self, Write};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use thiserror::Error;

/// Longest text sent through OSC 52; terminals drop or truncate much more.
const OSC52_LIMIT: usize = 1_000_000;

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("the system clipboard cannot be read here")]
    Unreadable,
    #[error("the system clipboard cannot be used here")]
    Unavailable,
    #[error("text is too large for the terminal clipboard ({0} bytes)")]
    TooLarge(usize),
    #[error("clipboard: {0}")]
    Backend(#[from] arboard::Error),
    #[error("clipboard: {0}")]
    Io(#[from] io::Error),
}

/// Test double with switches for what works.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Memory {
    pub(crate) content: Option<String>,
    pub(crate) readable: bool,
    pub(crate) writable: bool,
}

#[cfg(test)]
impl Default for Memory {
    fn default() -> Self {
        Self {
            content: None,
            readable: true,
            writable: true,
        }
    }
}

/// The operating system's clipboard, as far as we can reach it.
pub(crate) enum System {
    /// A real clipboard (X11, Wayland, Windows, macOS).
    Native(arboard::Clipboard),
    /// Write-only through the terminal: OSC 52 sets the clipboard of the
    /// machine the terminal runs on, even over SSH. Reading is not possible.
    Terminal,
    /// Nothing reachable; only the editor's own register works.
    Unavailable,
    #[cfg(test)]
    Memory(Memory),
}

impl System {
    /// The best clipboard this environment offers.
    pub(crate) fn detect() -> Self {
        match arboard::Clipboard::new() {
            Ok(clipboard) => Self::Native(clipboard),
            Err(_) => Self::Terminal,
        }
    }

    /// `Ok(None)` means the clipboard is readable but holds no text.
    pub(crate) fn read(&mut self) -> Result<Option<String>, ClipboardError> {
        match self {
            Self::Native(clipboard) => match clipboard.get_text() {
                Ok(text) => Ok(Some(text)),
                Err(arboard::Error::ContentNotAvailable) => Ok(None),
                Err(error) => Err(error.into()),
            },
            Self::Terminal | Self::Unavailable => Err(ClipboardError::Unreadable),
            #[cfg(test)]
            Self::Memory(memory) if memory.readable => Ok(memory.content.clone()),
            #[cfg(test)]
            Self::Memory(_) => Err(ClipboardError::Unreadable),
        }
    }

    pub(crate) fn write(&mut self, text: &str) -> Result<(), ClipboardError> {
        match self {
            Self::Native(clipboard) => Ok(clipboard.set_text(text)?),
            Self::Terminal => write_osc52(&mut io::stdout(), text),
            Self::Unavailable => Err(ClipboardError::Unavailable),
            #[cfg(test)]
            Self::Memory(memory) if memory.writable => {
                memory.content = Some(text.to_owned());
                Ok(())
            }
            #[cfg(test)]
            Self::Memory(_) => Err(ClipboardError::Unavailable),
        }
    }
}

/// Sets the clipboard of the terminal's machine: `ESC ] 52 ; c ; <base64> BEL`.
pub(super) fn write_osc52(out: &mut impl Write, text: &str) -> Result<(), ClipboardError> {
    if text.len() > OSC52_LIMIT {
        return Err(ClipboardError::TooLarge(text.len()));
    }
    let encoded = STANDARD.encode(text);
    write!(out, "\x1b]52;c;{encoded}\x07")?;
    out.flush()?;
    Ok(())
}
