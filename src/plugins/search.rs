//! The search of the plugin manager: a box of words, in which `tag:syntax` limits the list
//! to the plugins that have that tag (see `tags`).
//!
//! Every term must hold, words and tags alike (AND): `tag:git status` finds the plugins that
//! are tagged `git` and have `status` in their name, id, description or tags. A word that is
//! the name of a tag finds the plugins with it, so `rust` is enough. A `tag:` that is not in the
//! closed set matches nothing and is said, so that a mistyped tag is not mistaken for an empty
//! list.
//!
//! The note for a language is derived from [`Language`], so it cannot go out of step with the
//! grammars the editor has: the names and the extensions are the ones that open a file.

use crate::syntax::Language;

use super::tags::{Kind, Tag};

/// Words that say what a search for a language is after, and name no language.
const FILLER: [&str; 4] = ["syntax", "highlighting", "grammar", "language"];

/// What a search can match in a plugin.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Subject<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub description: &'a str,
    pub tags: &'a [Tag],
}

/// What was typed in the box, taken apart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Query {
    /// In lower case.
    words: Vec<String>,
    tags: Vec<Tag>,
    /// The names after `tag:` that are not in the set, as typed.
    unknown: Vec<String>,
}

impl Query {
    pub(crate) fn parse(text: &str) -> Self {
        let mut query = Self::default();
        for term in text.split_whitespace() {
            let Some(name) = tag_term(term) else {
                query.words.push(term.to_lowercase());
                continue;
            };
            // `tag:` with nothing after it is being typed: it limits nothing yet.
            match (name.is_empty(), Tag::parse(name)) {
                (true, _) => {}
                (false, Some(tag)) if !query.tags.contains(&tag) => query.tags.push(tag),
                (false, Some(_)) => {}
                (false, None) => query.unknown.push(name.to_owned()),
            }
        }
        query
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.words.is_empty() && self.tags.is_empty() && self.unknown.is_empty()
    }

    /// The names after `tag:` that are not tags, as typed.
    pub(crate) fn unknown_tags(&self) -> &[String] {
        &self.unknown
    }

    /// Whether every term holds for the plugin.
    pub(crate) fn matches(&self, subject: Subject<'_>) -> bool {
        if !self.unknown.is_empty() {
            return false;
        }
        if !self.tags.iter().all(|tag| subject.tags.contains(tag)) {
            return false;
        }
        let names: Vec<&str> = subject.tags.iter().map(|tag| tag.name()).collect();
        let haystack = format!(
            "{} {} {} {}",
            subject.id,
            subject.name,
            subject.description,
            names.join(" ")
        )
        .to_lowercase();
        self.words.iter().all(|word| haystack.contains(word))
    }

    /// What to tell about the languages the search is for: that the editor has the syntax of
    /// one built in, or (when `nothing_listed`) that one is not available yet. `None` when the
    /// search is not only about languages: a word that names none, or a tag that is not one,
    /// is a different question, which the list answers.
    pub(crate) fn language_note(&self, nothing_listed: bool) -> Option<String> {
        if !self.unknown.is_empty() {
            return None;
        }
        let mut languages: Vec<Named> = Vec::new();
        let terms = self
            .words
            .iter()
            .map(String::as_str)
            .chain(self.tags.iter().map(|tag| tag.name()));
        for term in terms {
            if FILLER.contains(&term) {
                continue;
            }
            let named = named_language(term)?;
            if !languages.contains(&named) {
                languages.push(named);
            }
        }
        let sentences: Vec<String> = languages
            .iter()
            .filter_map(|language| match (language.built_in, nothing_listed) {
                (true, _) => Some(format!("{} syntax is built in", language.name)),
                (false, true) => Some(format!("{} syntax is not available yet", language.name)),
                (false, false) => None,
            })
            .collect();
        (!sentences.is_empty()).then(|| sentences.join("; "))
    }
}

/// A language a search names.
#[derive(Debug, PartialEq, Eq)]
struct Named {
    name: String,
    built_in: bool,
}

/// The language a term is: one the editor has a grammar for (by name or extension, so `ts`,
/// `mjs` and `md` count), else a language of the set of tags.
fn named_language(term: &str) -> Option<Named> {
    match Language::by_name(term) {
        Some(Language::Plugin(_)) | None => {}
        Some(language) => {
            return Some(Named {
                name: language.name().to_owned(),
                built_in: true,
            });
        }
    }
    let tag = Tag::parse(term).filter(|tag| tag.kind() == Kind::Language)?;
    Some(Named {
        name: capitalised(tag.name()),
        built_in: false,
    })
}

fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// What follows `tag:` in a term, whatever the case of the prefix.
fn tag_term(term: &str) -> Option<&str> {
    let (prefix, name) = term.split_at_checked(4)?;
    prefix.eq_ignore_ascii_case("tag:").then_some(name)
}

/// The tags to list under the box while it is typed in: all of them, or, while the last term is
/// a `tag:` being typed, the ones that start with what follows it.
pub(crate) fn tags_to_offer(text: &str) -> Vec<Tag> {
    let typed = text
        .rsplit(char::is_whitespace)
        .next()
        .and_then(tag_term)
        .map(str::to_lowercase);
    Tag::all()
        .iter()
        .copied()
        .filter(|tag| {
            typed
                .as_ref()
                .is_none_or(|typed| tag.name().starts_with(typed))
        })
        .collect()
}
