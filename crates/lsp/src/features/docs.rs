//! The documentation of an item, which a server writes in markdown, as plain
//! text for a small box: no marks for code, bold, headings or links, and no
//! more than one blank line in a row.

/// `markdown` as plain text.
pub fn plain_text(markdown: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in markdown.replace("\r\n", "\n").lines() {
        let trimmed = line.trim_start();
        // The lines that open and close a block of code are not text.
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            continue;
        }
        // A rule between two parts is a blank line.
        let rule = trimmed.len() >= 3
            && ['-', '*', '_']
                .iter()
                .any(|mark| trimmed.chars().all(|c| c == *mark));
        let text = if rule { "" } else { line.trim_end() };
        let text = match text.trim_start().strip_prefix('#') {
            Some(rest) => rest.trim_start_matches('#').trim_start(),
            None => text,
        };
        let text = strip_inline(text);
        let blank = text.trim().is_empty();
        if blank && lines.last().is_none_or(|last| last.is_empty()) {
            continue;
        }
        lines.push(if blank { String::new() } else { text });
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}

/// `line` without backticks and emphasis marks, with a link as its text.
fn strip_inline(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '`' => at += 1,
            '*' | '_' if chars.get(at + 1) == Some(&chars[at]) => at += 2,
            '[' => match link_at(&chars, at) {
                Some((text, end)) => {
                    out.push_str(&text);
                    at = end;
                }
                None => {
                    out.push('[');
                    at += 1;
                }
            },
            other => {
                out.push(other);
                at += 1;
            }
        }
    }
    out
}

/// The text of the link `[text](target)` or `[text][reference]` that starts at
/// `chars[open]`, and the index after it.
fn link_at(chars: &[char], open: usize) -> Option<(String, usize)> {
    let close = (open + 1..chars.len()).find(|&at| chars[at] == ']')?;
    let closing = match chars.get(close + 1)? {
        '(' => ')',
        '[' => ']',
        _ => return None,
    };
    let end = (close + 2..chars.len()).find(|&at| chars[at] == closing)?;
    let text: String = chars[open + 1..close]
        .iter()
        .filter(|c| **c != '`')
        .collect();
    Some((text, end + 1))
}
