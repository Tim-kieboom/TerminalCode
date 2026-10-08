//! The editor's register plus a best-effort link to the system clipboard.

#[cfg(test)]
pub(crate) use system::ClipboardError;
#[cfg(test)]
pub(crate) use system::Memory;
pub(crate) use system::System;

mod system;
#[cfg(test)]
mod tests;

/// How copied text is meant to be pasted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegisterKind {
    /// Inserted at the cursor, replacing the selection.
    Charwise,
    /// Whole lines, each ending in a line break; inserted above the cursor's
    /// line. Copying with no selection produces this.
    Linewise,
}

/// Copied text and how to paste it. The system clipboard only carries the
/// text, so the kind lives here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Register {
    text: String,
    kind: RegisterKind,
}

impl Register {
    pub(crate) fn charwise(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: RegisterKind::Charwise,
        }
    }

    pub(crate) fn linewise(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: RegisterKind::Linewise,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn kind(&self) -> RegisterKind {
        self.kind
    }
}

/// Something worth telling the user about a paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Notice {
    /// The system clipboard could not be read, so the editor's own copy was
    /// pasted. Shown once.
    SystemUnreadable,
}

/// What a paste should insert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fetched {
    pub(crate) register: Register,
    pub(crate) notice: Option<Notice>,
}

/// The editor's own register, mirrored to the system clipboard when possible.
///
/// Copying always fills the register and tries to write the system clipboard.
/// Pasting reads the system clipboard: if it holds something other than what
/// we last wrote, another program changed it and that text wins; otherwise
/// (same text, empty, or unreadable) the register is used, which keeps its
/// kind.
pub(crate) struct Clipboard {
    register: Option<Register>,
    last_written: Option<String>,
    system: System,
    told_unreadable: bool,
}

impl Clipboard {
    pub(crate) fn new(system: System) -> Self {
        Self {
            register: None,
            last_written: None,
            system,
            told_unreadable: false,
        }
    }

    /// Only the editor's own register; used where touching the real clipboard
    /// would be wrong, such as tests.
    pub(crate) fn internal_only() -> Self {
        Self::new(System::Unavailable)
    }

    pub(crate) fn set(&mut self, register: Register) {
        // Remember what we tried to write even if it failed: a read that
        // differs from it must come from somewhere else.
        self.last_written = Some(register.text().to_owned());
        let _ = self.system.write(register.text());
        self.register = Some(register);
    }

    pub(crate) fn get(&mut self) -> Option<Fetched> {
        let (foreign, notice) = match self.system.read() {
            Ok(Some(text)) if self.last_written.as_deref() != Some(text.as_str()) => {
                (Some(text), None)
            }
            Ok(_) => (None, None),
            Err(_) => (None, self.unreadable_notice()),
        };

        let register = match foreign {
            Some(text) => Register::charwise(text),
            None => self.register.clone()?,
        };
        Some(Fetched { register, notice })
    }

    fn unreadable_notice(&mut self) -> Option<Notice> {
        if self.told_unreadable || self.register.is_none() {
            return None;
        }
        self.told_unreadable = true;
        Some(Notice::SystemUnreadable)
    }
}

impl std::fmt::Debug for Clipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clipboard")
            .field("register", &self.register)
            .finish_non_exhaustive()
    }
}
