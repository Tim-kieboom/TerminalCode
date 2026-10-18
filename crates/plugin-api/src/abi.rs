//! The version of the messages, which a plugin names in its manifest.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// The version of the messages a plugin was built for, as `major.minor`. The
/// minor version grows when something is added; the major when something that
/// an old plugin relies on is removed or changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Abi {
    pub major: u32,
    pub minor: u32,
}

/// Why the editor will not run a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AbiError {
    /// Built for a later version than this editor has.
    #[error(
        "it needs plugin API {plugin}, and this version of TerminalCode has {host}: update TerminalCode"
    )]
    TooNew { plugin: Abi, host: Abi },
    /// Built for an earlier major version, whose messages are gone.
    #[error(
        "it was built for plugin API {plugin}, which this version of TerminalCode (API {host}) no longer has: ask its author for an update"
    )]
    TooOld { plugin: Abi, host: Abi },
    #[error("`{0}` is not a plugin API version such as `1.0`")]
    Unreadable(String),
}

impl Abi {
    /// The version of the messages in this crate.
    pub const CURRENT: Self = Self::new(1, 0);

    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    /// Whether the editor, which speaks `self`, may run a plugin built for `plugin`:
    /// the same major version, and a minor version that is not later (the editor
    /// does not know what a later minor added, and the plugin may rely on it).
    /// An earlier minor is fine: everything it uses is still there.
    pub fn check(self, plugin: Self) -> Result<(), AbiError> {
        if plugin.major > self.major || (plugin.major == self.major && plugin.minor > self.minor) {
            return Err(AbiError::TooNew { plugin, host: self });
        }
        if plugin.major < self.major {
            return Err(AbiError::TooOld { plugin, host: self });
        }
        Ok(())
    }
}

impl fmt::Display for Abi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

impl FromStr for Abi {
    type Err = AbiError;

    /// Reads `major.minor`, such as `1.0`; a bare `1` is `1.0`.
    fn from_str(text: &str) -> Result<Self, AbiError> {
        let unreadable = || AbiError::Unreadable(text.to_owned());
        let (major, minor) = match text.split_once('.') {
            Some((major, minor)) => (major, minor),
            None => (text, "0"),
        };
        let number = |part: &str| part.parse::<u32>().map_err(|_| unreadable());
        Ok(Self::new(number(major)?, number(minor)?))
    }
}
