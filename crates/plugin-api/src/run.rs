//! The allowlist of a plugin that runs commands (`process.run`): the manifest lists
//! patterns, each a program and what its arguments may be, and the editor refuses
//! every command no pattern matches.
//!
//! ```toml
//! capabilities = ["process.run"]
//! run = ["cargo nextest *", "cargo llvm-cov nextest --junit-path *"]
//! ```
//!
//! A pattern is words separated by spaces. The first is the program (a name looked
//! up on `PATH`: no path, no `*`). Every other word is an argument that must be
//! equal; a `*` is any one argument, and a `*` that is the last word is any
//! arguments at all, none included. Nothing runs through a shell, so no argument
//! is ever read as more than itself. The pattern is shown to the user as written
//! when they are asked to allow the plugin.

use thiserror::Error;

/// Why a pattern is not one.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PatternError {
    #[error("a pattern has no program")]
    Empty,
    #[error("`{0}` is not a program name: no path, no `*`, and not an option")]
    BadProgram(String),
    #[error("`{0}` has a `*` among other characters: a `*` is a whole word")]
    PartialWildcard(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Word {
    Is(String),
    Any,
}

/// One entry of the allowlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPattern {
    text: String,
    program: String,
    words: Vec<Word>,
    /// Whether the last word is `*`: more arguments, of any number, may follow.
    rest: bool,
}

impl RunPattern {
    /// Reads a pattern as the manifest writes it.
    pub fn parse(text: &str) -> Result<Self, PatternError> {
        let mut parts = text.split_whitespace();
        let program = parts.next().ok_or(PatternError::Empty)?;
        let bad_program = program.contains(['*', '/', '\\']) || program.starts_with('-');
        if bad_program {
            return Err(PatternError::BadProgram(program.to_owned()));
        }
        let mut words = Vec::new();
        for part in parts {
            match part {
                "*" => words.push(Word::Any),
                other if other.contains('*') => {
                    return Err(PatternError::PartialWildcard(other.to_owned()));
                }
                other => words.push(Word::Is(other.to_owned())),
            }
        }
        let rest = words.last() == Some(&Word::Any);
        if rest {
            words.pop();
        }
        Ok(Self {
            text: parts_text(program, &words, rest),
            program: program.to_owned(),
            words,
            rest,
        })
    }

    /// The pattern as shown to the user.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether running `program` with `args` is what the pattern allows.
    pub fn matches(&self, program: &str, args: &[String]) -> bool {
        if program != self.program {
            return false;
        }
        let fits = match self.rest {
            true => args.len() >= self.words.len(),
            false => args.len() == self.words.len(),
        };
        fits && self.words.iter().zip(args).all(|(word, arg)| match word {
            Word::Is(text) => text == arg,
            Word::Any => true,
        })
    }
}

fn parts_text(program: &str, words: &[Word], rest: bool) -> String {
    let mut text = program.to_owned();
    for word in words {
        text.push(' ');
        match word {
            Word::Is(word) => text.push_str(word),
            Word::Any => text.push('*'),
        }
    }
    if rest {
        text.push_str(" *");
    }
    text
}

/// Whether running `program` with `args` is allowed by any of `patterns`.
pub fn allows(patterns: &[RunPattern], program: &str, args: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| pattern.matches(program, args))
}

/// Reads the patterns of a manifest.
pub fn parse_all<'a>(
    texts: impl IntoIterator<Item = &'a String>,
) -> Result<Vec<RunPattern>, PatternError> {
    texts
        .into_iter()
        .map(|text| RunPattern::parse(text))
        .collect()
}
