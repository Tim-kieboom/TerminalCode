use std::ops::Range;

use super::{Highlighter, Language, Span, SyntaxError};
use crate::buffer::Buffer;
use crate::ui::theme::Theme;

/// What highlights one document: the highlighter for its language, kept in
/// step with the text, and the spans for the parts of it that are on screen.
#[derive(Debug, Default)]
pub(crate) struct DocumentSyntax {
    current: Option<Current>,
    /// The language that could not be set up, so it is not retried every frame.
    failed: Option<Language>,
    error: Option<String>,
}

#[derive(Debug)]
struct Current {
    language: Language,
    highlighter: Highlighter,
    /// The buffer version the tree was parsed from.
    parsed: Option<u64>,
    /// What `spans` was computed for: the buffer version and the ranges.
    computed: Option<(u64, Vec<Range<usize>>)>,
    spans: Vec<Span>,
}

impl DocumentSyntax {
    /// Brings the spans up to date for `buffer` and the byte `ranges` to show.
    /// The language comes from the buffer's path, so a rename that changes the
    /// extension changes it here. Text that changed is parsed again, in full.
    pub(crate) fn update(&mut self, buffer: &Buffer, theme: &Theme, ranges: &[Range<usize>]) {
        let language = buffer.path().and_then(Language::from_path);
        if self.current.as_ref().map(|current| current.language) != language {
            self.current = None;
        }
        let Some(language) = language else {
            self.failed = None;
            self.error = None;
            return;
        };
        if self.failed == Some(language) {
            return;
        }
        if self.current.is_none() {
            match Current::new(language, theme) {
                Ok(current) => self.current = Some(current),
                Err(error) => return self.fail(language, error),
            }
        }
        let Some(current) = self.current.as_mut() else {
            return;
        };
        if let Err(error) = current.update(buffer, ranges) {
            self.current = None;
            self.fail(language, error);
        }
    }

    fn fail(&mut self, language: Language, error: SyntaxError) {
        self.failed = Some(language);
        self.error = Some(error.to_string());
    }

    /// Why highlighting could not be set up, once; asking again gives `None`
    /// until it fails anew.
    pub(crate) fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// The spans for the ranges last asked for, in order, not overlapping.
    pub(crate) fn spans(&self) -> &[Span] {
        self.current
            .as_ref()
            .map_or(&[][..], |current| &current.spans)
    }
}

impl Current {
    fn new(language: Language, theme: &Theme) -> Result<Self, SyntaxError> {
        Ok(Self {
            language,
            highlighter: Highlighter::new(language, theme)?,
            parsed: None,
            computed: None,
            spans: Vec::new(),
        })
    }

    fn update(&mut self, buffer: &Buffer, ranges: &[Range<usize>]) -> Result<(), SyntaxError> {
        let version = buffer.version();
        let up_to_date = self
            .computed
            .as_ref()
            .is_some_and(|(computed, wanted)| *computed == version && wanted == ranges);
        if up_to_date {
            return Ok(());
        }
        let text = buffer.text();
        if self.parsed != Some(version) {
            self.highlighter.parse(&text)?;
            self.parsed = Some(version);
        }
        self.spans = ranges
            .iter()
            .flat_map(|range| self.highlighter.spans(&text, range.clone()))
            .collect();
        self.computed = Some((version, ranges.to_vec()));
        Ok(())
    }
}
