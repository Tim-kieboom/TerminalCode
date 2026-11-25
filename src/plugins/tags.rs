//! The tags a plugin can have, to find it by: a closed set that the editor owns.
//!
//! A tag says what the plugin is (`git`, `syntax`) or which programming language it is for
//! (`rust`, `c++`). The manifest lists them (`tags = ["syntax", "rust"]`) and an index entry
//! copies them. A name outside the set is dropped without a word, in a manifest and in an
//! index alike, so that nobody can invent a category (`official`) and one thing is not three
//! tags (`git`, `Git`, `vcs`): a plugin of a new kind has no tag until an editor release adds
//! one.

/// What a tag tells about a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// What the plugin is or does.
    Purpose,
    /// The programming language it is for.
    Language,
}

/// A tag of the closed set; only [`Tag::parse`] makes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tag {
    name: &'static str,
    kind: Kind,
}

/// The set, in the order tags are shown: what a plugin is first, then its languages.
const KNOWN: &[Tag] = &[
    Tag::new("syntax", Kind::Purpose),
    Tag::new("git", Kind::Purpose),
    Tag::new("test", Kind::Purpose),
    Tag::new("language-server", Kind::Purpose),
    Tag::new("bend", Kind::Language),
    Tag::new("c", Kind::Language),
    Tag::new("c++", Kind::Language),
    Tag::new("c#", Kind::Language),
    Tag::new("css", Kind::Language),
    Tag::new("go", Kind::Language),
    Tag::new("html", Kind::Language),
    Tag::new("java", Kind::Language),
    Tag::new("javascript", Kind::Language),
    Tag::new("json", Kind::Language),
    Tag::new("kotlin", Kind::Language),
    Tag::new("lua", Kind::Language),
    Tag::new("markdown", Kind::Language),
    Tag::new("nix", Kind::Language),
    Tag::new("ocaml", Kind::Language),
    Tag::new("php", Kind::Language),
    Tag::new("python", Kind::Language),
    Tag::new("ruby", Kind::Language),
    Tag::new("rust", Kind::Language),
    Tag::new("shell", Kind::Language),
    Tag::new("sol", Kind::Language),
    Tag::new("sql", Kind::Language),
    Tag::new("swift", Kind::Language),
    Tag::new("toml", Kind::Language),
    Tag::new("typescript", Kind::Language),
    Tag::new("yaml", Kind::Language),
    Tag::new("zig", Kind::Language),
];

impl Tag {
    const fn new(name: &'static str, kind: Kind) -> Self {
        Self { name, kind }
    }

    /// The tag called `name`, ignoring case and the space around it; `None` for a name that
    /// is not in the set.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        let name = name.trim();
        KNOWN
            .iter()
            .copied()
            .find(|tag| tag.name.eq_ignore_ascii_case(name))
    }

    pub(crate) fn name(self) -> &'static str {
        self.name
    }

    pub(crate) fn kind(self) -> Kind {
        self.kind
    }

    /// Every tag of the set.
    pub(crate) fn all() -> &'static [Tag] {
        KNOWN
    }
}

/// The tags among `names`: each once, in the order of the set, the unknown ones left out.
pub(crate) fn known<S: AsRef<str>>(names: &[S]) -> Vec<Tag> {
    let found: Vec<Tag> = names
        .iter()
        .filter_map(|name| Tag::parse(name.as_ref()))
        .collect();
    KNOWN
        .iter()
        .copied()
        .filter(|tag| found.contains(tag))
        .collect()
}
