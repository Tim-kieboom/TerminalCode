//! Answers to what a program asks the terminal in its output.
//!
//! A program can ask where the cursor is by printing `ESC [ 6 n`; a terminal
//! answers with `ESC [ row ; column R`. Windows' pseudo-terminal (ConPTY) asks
//! this before it lets the shell start, so a terminal that never answers shows
//! nothing and never runs the shell.

/// The question "where is the cursor?".
const CURSOR_QUERY: &[u8] = b"\x1b[6n";

/// Feeds the child's output to the screen and says what is owed back.
#[derive(Debug, Default)]
pub(super) struct Replier {
    /// The end of the last output when it may be the start of a query that
    /// the next output finishes. Held back from the screen until then.
    held: Vec<u8>,
}

impl Replier {
    /// Puts `output` on `screen` and returns the answers to the queries in it,
    /// to be sent to the child. Each is answered with where the cursor was when
    /// the query came, not where the rest of the output leaves it.
    pub(super) fn feed(&mut self, screen: &mut vt100::Parser, output: &[u8]) -> Vec<u8> {
        let mut data = std::mem::take(&mut self.held);
        data.extend_from_slice(output);

        let mut replies = Vec::new();
        let mut start = 0;
        while let Some(found) = find(&data[start..], CURSOR_QUERY) {
            let end = start + found + CURSOR_QUERY.len();
            screen.process(&data[start..end]);
            let (row, column) = screen.screen().cursor_position();
            replies.extend_from_slice(format!("\x1b[{};{}R", row + 1, column + 1).as_bytes());
            start = end;
        }

        let rest = &data[start..];
        let waiting = partial_query_at_end(rest);
        let (shown, held) = rest.split_at(rest.len() - waiting);
        screen.process(shown);
        self.held = held.to_vec();
        replies
    }
}

/// Where `needle` first occurs in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// How many bytes at the end of `bytes` are the start, but not all, of a query.
fn partial_query_at_end(bytes: &[u8]) -> usize {
    (1..CURSOR_QUERY.len())
        .rev()
        .find(|&length| bytes.ends_with(&CURSOR_QUERY[..length]))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
