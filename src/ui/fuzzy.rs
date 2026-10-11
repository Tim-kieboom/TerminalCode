//! Fuzzy matching for the command palette and the file finder.

/// A query prepared for matching many texts: lowercased once, whitespace
/// dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Query {
    wanted: Vec<char>,
}

impl Query {
    pub(crate) fn new(query: &str) -> Self {
        let wanted = query
            .chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect();
        Self { wanted }
    }

    /// How well the query matches `text`, or `None` if it does not match at
    /// all. Every character of the query must appear in the text in order,
    /// ignoring case. Higher is better: runs of consecutive characters and
    /// matches at the start of a word score extra, and a match that starts
    /// early beats one that starts late.
    pub(crate) fn score(&self, text: &str) -> Option<i32> {
        let mut wanted = self.wanted.iter().peekable();
        if wanted.peek().is_none() {
            return Some(0);
        }

        let mut total = 0;
        let mut previous_matched = false;
        let mut previous: Option<char> = None;
        let mut first_match: Option<i32> = None;

        for (index, c) in text.chars().enumerate() {
            let Some(&&next) = wanted.peek() else { break };
            let matches = lowercase(c) == next;
            if matches {
                wanted.next();
                first_match.get_or_insert(index as i32);
                total += 10;
                if previous_matched {
                    total += 15;
                }
                if previous.is_none_or(|p| !p.is_alphanumeric()) {
                    total += 20;
                }
            }
            previous_matched = matches;
            previous = Some(c);
        }

        match wanted.peek() {
            Some(_) => None,
            None => Some(total - first_match.unwrap_or(0)),
        }
    }
}

impl Query {
    /// The character positions (counted in characters, from 0) of `text` that the
    /// match uses, in order: the same ones [`Query::score`] counts. `None` when
    /// the query does not match; empty for an empty query.
    pub(crate) fn positions(&self, text: &str) -> Option<Vec<usize>> {
        let mut wanted = self.wanted.iter().peekable();
        let mut found = Vec::with_capacity(self.wanted.len());
        for (index, c) in text.chars().enumerate() {
            let Some(&&next) = wanted.peek() else { break };
            if lowercase(c) == next {
                wanted.next();
                found.push(index);
            }
        }
        wanted.peek().is_none().then_some(found)
    }
}

/// The lowercase form of `c`; for the rare character whose lowercase is
/// several characters, the first of them.
fn lowercase(c: char) -> char {
    match c.is_ascii() {
        true => c.to_ascii_lowercase(),
        false => c.to_lowercase().next().unwrap_or(c),
    }
}

/// [`Query::score`] for a query used once.
pub(crate) fn score(query: &str, text: &str) -> Option<i32> {
    Query::new(query).score(text)
}
