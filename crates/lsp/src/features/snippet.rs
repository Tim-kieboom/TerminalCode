//! The text a server gives for an item when it is a snippet: `push(${1:value})`,
//! `len()$0`, `HashMap<$0>`. This client has no tab stops to move between: it
//! writes the text with the default text of its placeholders in it (`push(value)`)
//! and leaves the first stop selected, so that typing replaces it; a stop with no
//! text is a cursor. After the first stop, or `$0` when there is none, nothing is
//! kept of the others.

/// A snippet written out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    pub text: String,
    /// What is selected, as byte offsets into `text`: `anchor..cursor`, empty for
    /// a cursor.
    pub anchor: usize,
    pub cursor: usize,
}

/// A tab stop: its number and the text it covers.
struct Stop {
    number: u32,
    start: usize,
    end: usize,
}

/// `snippet` as text. After each line break `indent` is written, since a
/// server writes the lines of a snippet as if the first line were at the start
/// of the line, and the client is to indent them like the line they go into.
pub fn expand(snippet: &str, indent: &str) -> Expanded {
    let chars: Vec<char> = snippet.chars().collect();
    let mut out = String::new();
    let mut stops = Vec::new();
    let mut at = 0;
    emit(&chars, &mut at, &mut out, &mut stops, indent, false);
    // The first stop in order, then `$0`, then the end.
    let first = stops
        .iter()
        .filter(|stop| stop.number > 0)
        .min_by_key(|stop| (stop.number, stop.start))
        .or_else(|| stops.iter().find(|stop| stop.number == 0));
    let (anchor, cursor) = first.map_or((out.len(), out.len()), |stop| (stop.start, stop.end));
    Expanded {
        text: out,
        anchor,
        cursor,
    }
}

/// Writes the text from `chars[*at]` on to `out`, up to the end or, inside a
/// default (`in_default`), up to the `}` that closes it (which is left for the caller).
fn emit(
    chars: &[char],
    at: &mut usize,
    out: &mut String,
    stops: &mut Vec<Stop>,
    indent: &str,
    in_default: bool,
) {
    while let Some(&c) = chars.get(*at) {
        match c {
            '}' if in_default => return,
            '\\' if chars
                .get(*at + 1)
                .is_some_and(|next| matches!(next, '$' | '}' | '\\')) =>
            {
                out.push(chars[*at + 1]);
                *at += 2;
            }
            '$' => dollar(chars, at, out, stops, indent),
            '\n' => {
                out.push('\n');
                out.push_str(indent);
                *at += 1;
            }
            other => {
                out.push(other);
                *at += 1;
            }
        }
    }
}

/// Reads what follows the `$` at `chars[*at]`: a tab stop (`$1`, `${1}`,
/// `${1:default}`, `${1|a,b|}`), a variable (`$NAME`, `${NAME:default}`, which
/// is its default or nothing), or a `$` that is neither and stays.
fn dollar(chars: &[char], at: &mut usize, out: &mut String, stops: &mut Vec<Stop>, indent: &str) {
    let after = *at + 1;
    match chars.get(after) {
        Some(c) if c.is_ascii_digit() => {
            let (number, end) = number_at(chars, after);
            stops.push(Stop {
                number,
                start: out.len(),
                end: out.len(),
            });
            *at = end;
        }
        Some('{') => {
            *at = after + 1;
            braced(chars, at, out, stops, indent);
        }
        Some(c) if c.is_ascii_alphabetic() || *c == '_' => {
            *at = skip_name(chars, after);
        }
        _ => {
            out.push('$');
            *at = after;
        }
    }
}

/// What is inside `${...}`, with `chars[*at]` just after the `{`; leaves `*at`
/// after the closing `}`.
fn braced(chars: &[char], at: &mut usize, out: &mut String, stops: &mut Vec<Stop>, indent: &str) {
    let numbered = chars.get(*at).is_some_and(char::is_ascii_digit);
    let number = match numbered {
        true => {
            let (number, end) = number_at(chars, *at);
            *at = end;
            Some(number)
        }
        false => {
            *at = skip_name(chars, *at);
            None
        }
    };
    let start = out.len();
    match chars.get(*at) {
        Some(':') => {
            *at += 1;
            emit(chars, at, out, stops, indent, true);
        }
        Some('|') => {
            *at += 1;
            choice(chars, at, out);
        }
        _ => {}
    }
    // Up to and including the `}`; an unclosed one runs to the end.
    while let Some(&c) = chars.get(*at) {
        *at += 1;
        if c == '}' {
            break;
        }
    }
    if let Some(number) = number {
        stops.push(Stop {
            number,
            start,
            end: out.len(),
        });
    }
}

/// The first of the choices `a,b,c|` that follow `${1|`; leaves `*at` at the `}`.
fn choice(chars: &[char], at: &mut usize, out: &mut String) {
    let mut first = true;
    while let Some(&c) = chars.get(*at) {
        match c {
            '|' => {
                *at += 1;
                return;
            }
            ',' => first = false,
            '\\' if chars.get(*at + 1).is_some() => {
                *at += 1;
                if first {
                    out.push(chars[*at]);
                }
            }
            other if first => out.push(other),
            _ => {}
        }
        *at += 1;
    }
}

/// The number that starts at `chars[at]`, and the index after it.
fn number_at(chars: &[char], at: usize) -> (u32, usize) {
    let mut end = at;
    let mut number: u32 = 0;
    while let Some(digit) = chars.get(end).and_then(|c| c.to_digit(10)) {
        number = number.saturating_mul(10).saturating_add(digit);
        end += 1;
    }
    (number, end)
}

/// The index after the name (letters, digits, underscores) at `chars[at]`.
fn skip_name(chars: &[char], at: usize) -> usize {
    let mut end = at;
    while chars
        .get(end)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
    {
        end += 1;
    }
    end
}
