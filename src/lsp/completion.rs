//! Asking a server what could be typed at a position, and reading what it
//! answers: the request, the list of items with their edits, and what the
//! server says it can do (`completionProvider`). The positions stay as the
//! server sent them; turning them into bytes is for whoever has the text.
//!
//! The client declares no snippet support and no lazy resolving of items, so a
//! server sends plain text and puts the edits an item needs (the `use` line of
//! an auto-import) in the item itself.

use std::time::Duration;

use serde_json::{Value, json};

use super::position::LspPosition;
use super::{Connection, LspError, PendingReply};

const METHOD: &str = "textDocument/completion";

/// What made the client ask. The numbers are the protocol's `triggerKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// A word is being typed (or the box was asked for).
    Typing,
    /// A trigger character the server named was typed.
    Character(char),
    /// The last list was incomplete and the word has grown.
    Incomplete,
}

impl Trigger {
    fn context(self) -> Value {
        match self {
            Self::Typing => json!({ "triggerKind": 1 }),
            Self::Character(character) => {
                json!({ "triggerKind": 2, "triggerCharacter": character.to_string() })
            }
            Self::Incomplete => json!({ "triggerKind": 3 }),
        }
    }
}

/// What the server can do for completion, from its `completionProvider`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Provider {
    /// Characters that make the box open by themselves (`.`, `::` gives `:`).
    pub(crate) trigger_characters: Vec<char>,
}

impl Provider {
    /// `None` for a server that does not offer completion.
    pub(crate) fn from_capabilities(capabilities: &Value) -> Option<Self> {
        let provider = capabilities.get("completionProvider")?;
        let trigger_characters = provider
            .get("triggerCharacters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            // A trigger is one character; anything longer is not one.
            .filter_map(|text| {
                let mut chars = text.chars();
                chars.next().filter(|_| chars.next().is_none())
            })
            .collect();
        // `true` is not in the protocol but is what a sloppy server may say.
        match provider {
            Value::Object(_) | Value::Bool(true) => Some(Self { trigger_characters }),
            _ => None,
        }
    }

    pub(crate) fn triggers_on(&self, character: char) -> bool {
        self.trigger_characters.contains(&character)
    }
}

/// What kind of thing an item is, from the protocol's `CompletionItemKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Text,
    Method,
    Function,
    Constructor,
    Field,
    Variable,
    Class,
    Interface,
    Module,
    Property,
    Unit,
    Value,
    Enum,
    Keyword,
    Snippet,
    Color,
    File,
    Reference,
    Folder,
    EnumMember,
    Constant,
    Struct,
    Event,
    Operator,
    TypeParameter,
    /// A server that names none, or one this client does not know.
    Other,
}

impl Kind {
    fn from_code(code: Option<u64>) -> Self {
        match code {
            Some(1) => Self::Text,
            Some(2) => Self::Method,
            Some(3) => Self::Function,
            Some(4) => Self::Constructor,
            Some(5) => Self::Field,
            Some(6) => Self::Variable,
            Some(7) => Self::Class,
            Some(8) => Self::Interface,
            Some(9) => Self::Module,
            Some(10) => Self::Property,
            Some(11) => Self::Unit,
            Some(12) => Self::Value,
            Some(13) => Self::Enum,
            Some(14) => Self::Keyword,
            Some(15) => Self::Snippet,
            Some(16) => Self::Color,
            Some(17) => Self::File,
            Some(18) => Self::Reference,
            Some(19) => Self::Folder,
            Some(20) => Self::EnumMember,
            Some(21) => Self::Constant,
            Some(22) => Self::Struct,
            Some(23) => Self::Event,
            Some(24) => Self::Operator,
            Some(25) => Self::TypeParameter,
            _ => Self::Other,
        }
    }

    /// The one letter the box shows next to the name.
    pub(crate) fn tag(self) -> char {
        match self {
            Self::Text => 't',
            Self::Method => 'm',
            Self::Function => 'f',
            Self::Constructor => 'n',
            Self::Field | Self::Property => 'p',
            Self::Variable => 'v',
            Self::Class => 'c',
            Self::Interface => 'i',
            Self::Module => 'M',
            Self::Unit => 'u',
            Self::Value => 'l',
            Self::Enum => 'e',
            Self::Keyword => 'k',
            Self::Snippet => '*',
            Self::Color => '#',
            Self::File => 'F',
            Self::Reference => 'r',
            Self::Folder => 'D',
            Self::EnumMember => 'E',
            Self::Constant => 'C',
            Self::Struct => 's',
            Self::Event => '!',
            Self::Operator => 'o',
            Self::TypeParameter => 'T',
            Self::Other => '?',
        }
    }
}

/// A replacement of the text between two positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextEdit {
    pub(crate) start: LspPosition,
    pub(crate) end: LspPosition,
    pub(crate) new_text: String,
}

/// One suggestion, as the server sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletionItem {
    /// What the box shows, and what is written when nothing says otherwise.
    pub(crate) label: String,
    pub(crate) kind: Kind,
    /// A short note beside the name, such as a signature.
    pub(crate) detail: Option<String>,
    /// What the list is ordered by; the label when the server gives none.
    pub(crate) sort_text: String,
    /// What the typed text is matched against; the label when the server gives none.
    pub(crate) filter_text: String,
    pub(crate) insert_text: Option<String>,
    /// What to write in place of the word, when the server says where.
    pub(crate) text_edit: Option<TextEdit>,
    /// Edits elsewhere that go with picking this item: the `use` line of an import.
    pub(crate) additional_edits: Vec<TextEdit>,
    /// Whether the text to write is a snippet (never asked for, but a server
    /// can send one anyway, and writing its `$1` as it is would be wrong).
    pub(crate) snippet: bool,
}

/// The answer to one request.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CompletionList {
    pub(crate) items: Vec<CompletionItem>,
    /// The list leaves out items that a longer word would have: ask again as it grows.
    pub(crate) incomplete: bool,
}

/// A completion request that was sent and has not been answered yet.
pub(crate) struct PendingCompletion(PendingReply);

impl PendingCompletion {
    /// The list, waiting at most `timeout` for it.
    pub(crate) fn wait(self, timeout: Duration) -> Result<CompletionList, LspError> {
        parse_completion(&self.0.wait(timeout)?)
    }
}

/// Asks the server what could be typed at `position` of the document `uri`.
pub(crate) fn request(
    connection: &Connection,
    uri: &str,
    position: LspPosition,
    trigger: Trigger,
) -> Result<PendingCompletion, LspError> {
    let params = json!({
        "textDocument": { "uri": uri },
        "position": { "line": position.line, "character": position.character },
        "context": trigger.context(),
    });
    connection.request(METHOD, params).map(PendingCompletion)
}

/// Reads the answer to `textDocument/completion`: nothing (`null`), a plain
/// list, or a list that says whether it is complete. An item that is not
/// usable (no label) is left out; an answer of any other shape is an error.
pub(crate) fn parse_completion(result: &Value) -> Result<CompletionList, LspError> {
    let unexpected = |detail: &str| LspError::Unexpected {
        method: METHOD.to_owned(),
        detail: detail.to_owned(),
    };
    let (items, incomplete) = match result {
        Value::Null => return Ok(CompletionList::default()),
        Value::Array(items) => (items, false),
        Value::Object(list) => {
            let items = list
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| unexpected("the list has no items"))?;
            let incomplete = list
                .get("isIncomplete")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            (items, incomplete)
        }
        _ => return Err(unexpected("it is neither a list nor null")),
    };
    Ok(CompletionList {
        items: items.iter().filter_map(parse_item).collect(),
        incomplete,
    })
}

fn parse_item(value: &Value) -> Option<CompletionItem> {
    let label = value.get("label")?.as_str()?.to_owned();
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    let detail = text("detail")
        .filter(|detail| !detail.is_empty())
        .or_else(|| {
            value
                .pointer("/labelDetails/detail")
                .and_then(Value::as_str)
                .filter(|detail| !detail.is_empty())
                .map(str::to_owned)
        });
    Some(CompletionItem {
        kind: Kind::from_code(value.get("kind").and_then(Value::as_u64)),
        detail,
        sort_text: text("sortText").unwrap_or_else(|| label.clone()),
        filter_text: text("filterText").unwrap_or_else(|| label.clone()),
        insert_text: text("insertText"),
        text_edit: value.get("textEdit").and_then(parse_main_edit),
        additional_edits: value
            .get("additionalTextEdits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(parse_edit)
            .collect(),
        snippet: value.get("insertTextFormat").and_then(Value::as_u64) == Some(2),
        label,
    })
}

/// The edit that writes the item: a plain edit, or an insert-or-replace one
/// (never asked for), of which the insert half is taken.
fn parse_main_edit(value: &Value) -> Option<TextEdit> {
    match value.get("insert") {
        Some(insert) => Some(TextEdit {
            start: position(insert.get("start")?)?,
            end: position(insert.get("end")?)?,
            new_text: value.get("newText")?.as_str()?.to_owned(),
        }),
        None => parse_edit(value),
    }
}

fn parse_edit(value: &Value) -> Option<TextEdit> {
    let range = value.get("range")?;
    Some(TextEdit {
        start: position(range.get("start")?)?,
        end: position(range.get("end")?)?,
        new_text: value.get("newText")?.as_str()?.to_owned(),
    })
}

fn position(value: &Value) -> Option<LspPosition> {
    Some(LspPosition {
        line: u32::try_from(value.get("line")?.as_u64()?).ok()?,
        character: u32::try_from(value.get("character")?.as_u64()?).ok()?,
    })
}
