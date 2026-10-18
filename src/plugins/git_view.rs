//! What a plugin sees of git: the state of the repository as pages of files, the
//! way the messages of the plugin API say them.

use crate::git::{Change, Entry, Kind, Status};
use crate::plugin_api::{GitEntry, GitKind, GitSection, GitSummary, MAX_PAGE, Refusal, ToPlugin};

/// The files of a state of the repository in each section, worked out once for
/// each state so that a page costs only copying it.
#[derive(Debug)]
pub(super) struct Sections {
    summary: GitSummary,
    staged: Vec<GitEntry>,
    changes: Vec<GitEntry>,
    untracked: Vec<GitEntry>,
    conflicts: Vec<GitEntry>,
}

impl Sections {
    pub(super) fn of(generation: u64, status: &Status) -> Self {
        let mut sections = Self {
            summary: GitSummary::empty(generation),
            staged: Vec::new(),
            changes: Vec::new(),
            untracked: Vec::new(),
            conflicts: Vec::new(),
        };
        for entry in &status.entries {
            sections.add(entry);
        }
        let branch = &status.branch;
        let summary = &mut sections.summary;
        summary.branch = branch.name.clone();
        summary.commit = branch.oid.clone();
        summary.upstream = branch.upstream.clone();
        summary.ahead = branch.ahead;
        summary.behind = branch.behind;
        summary.staged = sections.staged.len();
        summary.changes = sections.changes.len();
        summary.untracked = sections.untracked.len();
        summary.conflicts = sections.conflicts.len();
        sections
    }

    fn add(&mut self, entry: &Entry) {
        let make = |kind| {
            let mut wire = GitEntry::new(slashed(&entry.path), kind);
            wire.original = entry.original.as_deref().map(slashed);
            wire
        };
        match entry.change {
            Change::Tracked { staged, unstaged } => {
                if let Some(kind) = staged {
                    self.staged.push(make(kind_of(kind)));
                }
                if let Some(kind) = unstaged {
                    self.changes.push(make(kind_of(kind)));
                }
            }
            Change::Untracked => self.untracked.push(make(GitKind::Untracked)),
            Change::Conflicted => self.conflicts.push(make(GitKind::Conflicted)),
        }
    }

    pub(super) fn summary(&self) -> &GitSummary {
        &self.summary
    }

    /// The page of `section` from `offset`, at most `limit` entries (and
    /// [`MAX_PAGE`]), from this state, stamped with its generation.
    pub(super) fn page(
        &self,
        request: u64,
        section: GitSection,
        offset: usize,
        limit: usize,
    ) -> ToPlugin {
        let all = match section {
            GitSection::Staged => &self.staged,
            GitSection::Changes => &self.changes,
            GitSection::Untracked => &self.untracked,
            GitSection::Conflicts => &self.conflicts,
            _ => {
                return ToPlugin::Refused {
                    request,
                    reason: Refusal::BadArgument {
                        detail: "a section this version does not have".into(),
                    },
                };
            }
        };
        let start = offset.min(all.len());
        let end = start.saturating_add(limit.min(MAX_PAGE)).min(all.len());
        ToPlugin::GitEntries {
            request,
            generation: self.summary.generation,
            section,
            offset: start,
            total: all.len(),
            entries: all[start..end].to_vec(),
        }
    }
}

/// A path with `/` between its parts, as the messages say it on every platform.
fn slashed(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn kind_of(kind: Kind) -> GitKind {
    match kind {
        Kind::Modified => GitKind::Modified,
        Kind::TypeChanged => GitKind::TypeChanged,
        Kind::Added => GitKind::Added,
        Kind::Deleted => GitKind::Deleted,
        Kind::Renamed => GitKind::Renamed,
        Kind::Copied => GitKind::Copied,
    }
}
