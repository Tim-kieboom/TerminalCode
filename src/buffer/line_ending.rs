/// Line break style of a file, kept so that saving and new lines match the
/// original.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LineEnding {
    #[default]
    Lf,
    Crlf,
}

impl LineEnding {
    /// Style of the first line break in `text`; `Lf` when there is none.
    /// Files with mixed endings keep their text as is and report the first.
    pub(crate) fn detect(text: &str) -> Self {
        let Some(newline) = text.find('\n') else {
            return Self::Lf;
        };
        if text[..newline].ends_with('\r') {
            Self::Crlf
        } else {
            Self::Lf
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }
}
