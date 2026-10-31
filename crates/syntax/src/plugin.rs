//! Languages that plugins add: a tree-sitter grammar built for WebAssembly, the query that
//! names what to color, the extensions and how a line is commented. They are kept in a table
//! the whole program shares, since a [`Language`] is a small value that is copied about and
//! the grammar behind it is not.
//!
//! A language is named by a [`PluginSlot`]: where in the table, and which time it was put
//! there, so a plugin that is updated or turned off and on is another language to the
//! documents that had the old one, and they set themselves up again.

use std::sync::{Arc, OnceLock, RwLock};

use tree_sitter::wasmtime::Engine;
use tree_sitter::{Language as Grammar, Query, WasmStore};

use super::{Language, SyntaxError};

/// What a plugin tells the editor of its language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageSpec {
    /// What a person calls it.
    pub name: String,
    /// The extensions of its files, without the dot.
    pub extensions: Vec<String>,
    /// How a line is commented, when there is a way.
    pub comment: Option<String>,
    /// The name the grammar's entry in the module has (`tree_sitter_<name>`).
    pub entry: String,
    /// The grammar: a module built with `tree-sitter build --wasm`.
    pub grammar: Vec<u8>,
    /// The highlight query.
    pub highlights: String,
}

/// Which language of the table, and which time it was put there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PluginSlot {
    index: usize,
    generation: u64,
}

/// A language as the table keeps it. Names are leaked, once for each time a language is
/// registered: there are few, and a [`Language`] hands them out as `&'static str`.
pub(super) struct Entry {
    pub(super) name: &'static str,
    extensions: Vec<String>,
    pub(super) comment: Option<&'static str>,
    entry: String,
    grammar: Vec<u8>,
    pub(super) highlights: String,
}

struct Slot {
    owner: String,
    generation: u64,
    entry: Option<Arc<Entry>>,
}

#[derive(Default)]
struct Table {
    slots: Vec<Slot>,
    generations: u64,
}

fn table() -> &'static RwLock<Table> {
    static TABLE: OnceLock<RwLock<Table>> = OnceLock::new();
    TABLE.get_or_init(RwLock::default)
}

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(Engine::default)
}

/// Puts the language of `owner` (a plugin's id) in the table, in place of the one it had.
/// The grammar and the query are tried first, so a plugin that cannot work is told at once
/// and leaves the table as it was.
pub fn register(owner: &str, spec: LanguageSpec) -> Result<(), SyntaxError> {
    let name: &'static str = Box::leak(spec.name.clone().into_boxed_str());
    let entry = Entry {
        name,
        extensions: spec.extensions,
        comment: spec.comment.map(|text| &*Box::leak(text.into_boxed_str())),
        entry: spec.entry,
        grammar: spec.grammar,
        highlights: spec.highlights,
    };
    let (grammar, _store) = load(&entry)?;
    Query::new(&grammar, &entry.highlights).map_err(|source| SyntaxError::Query {
        language: name,
        source,
    })?;
    let mut table = table()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    table.generations += 1;
    let generation = table.generations;
    let entry = Some(Arc::new(entry));
    match table.slots.iter_mut().find(|slot| slot.owner == owner) {
        Some(slot) => {
            slot.generation = generation;
            slot.entry = entry;
        }
        None => table.slots.push(Slot {
            owner: owner.to_owned(),
            generation,
            entry,
        }),
    }
    Ok(())
}

/// Takes the language of `owner` out of the table; files of its kind are plain again.
pub fn unregister(owner: &str) {
    let mut table = table()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(slot) = table.slots.iter_mut().find(|slot| slot.owner == owner) {
        slot.entry = None;
    }
}

/// The language of the files with this extension, if a plugin added one.
pub(super) fn by_extension(extension: &str) -> Option<PluginSlot> {
    let table = table()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    table.slots.iter().enumerate().find_map(|(index, slot)| {
        let entry = slot.entry.as_ref()?;
        entry
            .extensions
            .iter()
            .any(|known| known == extension)
            .then_some(PluginSlot {
                index,
                generation: slot.generation,
            })
    })
}

/// The language in `slot`, unless it was replaced or removed since.
pub(super) fn get(slot: PluginSlot) -> Option<Arc<Entry>> {
    let table = table()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let found = table.slots.get(slot.index)?;
    (found.generation == slot.generation)
        .then(|| found.entry.clone())
        .flatten()
}

/// The grammar of `entry`, and the store it lives in, which the parser must be given before
/// the grammar.
pub(super) fn load(entry: &Entry) -> Result<(Grammar, WasmStore), SyntaxError> {
    let wasm = |source| SyntaxError::Wasm {
        language: entry.name,
        source,
    };
    let mut store = WasmStore::new(engine()).map_err(wasm)?;
    let grammar = store
        .load_language(&entry.entry, &entry.grammar)
        .map_err(wasm)?;
    Ok((grammar, store))
}

impl Language {
    pub(super) fn plugin_entry(slot: PluginSlot) -> Result<Arc<Entry>, SyntaxError> {
        get(slot).ok_or(SyntaxError::Gone)
    }
}
