//! What a finder query asks for: files matching some text, and perhaps a place
//! in the one that gets opened (`main.rs:42` or `main.rs:42:7`).

/// A query taken apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Parsed<'a> {
    /// What the file names are matched against.
    pub(crate) text: &'a str,
    /// The line to go to, counted from 1.
    pub(crate) line: Option<usize>,
    /// The column to go to, counted from 1; only with a line.
    pub(crate) column: Option<usize>,
}

/// Splits `query` into the text to match and a line and column, when it ends
/// in `:line` or `:line:column`. A lone trailing `:` is dropped from the text
/// (the number is on its way), and so is a `:` after a line. A number that is
/// part of a name (`foo2`) is not a line. A query that is only a place
/// (`:42`) is just text.
pub(crate) fn parse(query: &str) -> Parsed<'_> {
    let trimmed = query.trim_end();
    let without_colon = trimmed.strip_suffix(':').unwrap_or(trimmed);
    let Some((rest, last)) = without_colon.rsplit_once(':') else {
        return plain(without_colon);
    };
    let Some(last) = number(last) else {
        return plain(without_colon);
    };
    // `rest` may itself end in `:line`, making `last` the column.
    if let Some((text, line)) = rest.rsplit_once(':')
        && let Some(line) = number(line)
        && !text.is_empty()
    {
        return Parsed {
            text,
            line: Some(line),
            column: Some(last),
        };
    }
    match rest.is_empty() {
        true => plain(without_colon),
        false => Parsed {
            text: rest,
            line: Some(last),
            column: None,
        },
    }
}

fn plain(text: &str) -> Parsed<'_> {
    Parsed {
        text,
        line: None,
        column: None,
    }
}

/// A number written in digits; 0 counts as 1, since places start there.
fn number(text: &str) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().map(|n: usize| n.max(1))
}
