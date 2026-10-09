use std::ops::Range;

use super::mapping::map_spans;
use super::{Highlighter, Language, Span, SyntaxError};
use crate::buffer::{Buffer, EditInfo};
use crate::ui::theme::Theme;

/// More edits than this are not kept for the next parse: the tree is parsed
/// afresh instead, which is cheaper than replaying them anyway.
const MAX_PENDING_EDITS: usize = 10_000;

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
    /// The edits made since `parsed`, in order, as long as every change to the
    /// text since then is among them.
    pending: Vec<EditInfo>,
    /// More edits came than `pending` keeps.
    overflowed: bool,
    /// What `spans` was computed for: the buffer version and the ranges.
    computed: Option<(u64, Vec<Range<usize>>)>,
    spans: Vec<Span>,
    /// How often the text was parsed in full and from the previous tree.
    #[cfg(test)]
    parses: Parses,
}

#[cfg(test)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Parses {
    pub(crate) full: usize,
    pub(crate) incremental: usize,
}

impl DocumentSyntax {
    /// Notes edits made to the document's text, oldest first, so the next
    /// [`DocumentSyntax::update`] can parse from the previous tree. The spans
    /// follow along at once, so they never describe text that is gone.
    pub(crate) fn record_edits(&mut self, edits: &[EditInfo]) {
        let Some(current) = self.current.as_mut() else {
            return;
        };
        current.record(edits);
    }

    /// Brings the spans up to date for `buffer` and the byte `ranges` to show.
    /// The language comes from the buffer's path, so a rename that changes the
    /// extension changes it here. Text that changed is parsed again, from the
    /// previous tree when every change since is known (see
    /// [`DocumentSyntax::record_edits`]) and from scratch otherwise.
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

    #[cfg(test)]
    pub(crate) fn parses(&self) -> Parses {
        self.current
            .as_ref()
            .map(|current| current.parses)
            .unwrap_or_default()
    }
}

impl Current {
    fn new(language: Language, theme: &Theme) -> Result<Self, SyntaxError> {
        Ok(Self {
            language,
            highlighter: Highlighter::new(language, theme)?,
            parsed: None,
            pending: Vec::new(),
            overflowed: false,
            computed: None,
            spans: Vec::new(),
            #[cfg(test)]
            parses: Parses::default(),
        })
    }

    fn record(&mut self, edits: &[EditInfo]) {
        if self.parsed.is_none() || self.overflowed {
            return;
        }
        for edit in edits {
            map_spans(&mut self.spans, edit);
        }
        if self.pending.len() + edits.len() > MAX_PENDING_EDITS {
            self.pending.clear();
            self.overflowed = true;
            return;
        }
        self.pending.extend_from_slice(edits);
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
            self.parse(&text, version)?;
        }
        self.spans = ranges
            .iter()
            .flat_map(|range| self.highlighter.spans(&text, range.clone()))
            .collect();
        self.computed = Some((version, ranges.to_vec()));
        Ok(())
    }

    /// Parses `text`, the text of buffer `version`. Every edit applied
    /// changed the version by one, so the recorded ones lead from the parsed
    /// version to this one exactly when there are as many as the versions
    /// between; anything else (a reload, say) means they are not all known.
    fn parse(&mut self, text: &str, version: u64) -> Result<(), SyntaxError> {
        let known = !self.overflowed
            && self
                .parsed
                .is_some_and(|parsed| parsed + self.pending.len() as u64 == version);
        if known {
            self.highlighter.edit(&self.pending);
            self.highlighter.reparse(text)?;
            #[cfg(test)]
            {
                self.parses.incremental += 1;
            }
        } else {
            self.highlighter.parse(text)?;
            #[cfg(test)]
            {
                self.parses.full += 1;
            }
        }
        self.parsed = Some(version);
        self.pending.clear();
        self.overflowed = false;
        Ok(())
    }
}
