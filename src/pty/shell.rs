/// The program a terminal pane runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shell {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
}

impl Shell {
    /// The user's shell: `$SHELL` where there is one, else what the platform
    /// has (`sh` on Unix, `%COMSPEC%` or `cmd.exe` on Windows).
    pub(crate) fn detect() -> Self {
        let variable = if cfg!(windows) { "COMSPEC" } else { "SHELL" };
        Self::from_setting(std::env::var(variable).ok().as_deref())
    }

    /// The shell named by an environment setting, if it is not empty.
    pub(crate) fn from_setting(setting: Option<&str>) -> Self {
        let program = match setting.map(str::trim) {
            Some(program) if !program.is_empty() => program,
            _ if cfg!(windows) => "cmd.exe",
            _ => "/bin/sh",
        };
        Self::program(program)
    }

    pub(crate) fn program(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    /// A shell that runs `script` and exits, for tests.
    #[cfg(test)]
    pub(crate) fn script(script: &str) -> Self {
        Self {
            program: "sh".to_owned(),
            args: vec!["-c".to_owned(), script.to_owned()],
        }
    }
}
