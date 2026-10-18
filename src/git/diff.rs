//! A line diff of two texts, each given as the hash of each of its lines:
//! which lines of the new text were added or changed, and where lines of the
//! old one are gone. Myers' algorithm, after the lines both texts start and
//! end with are taken off, so a change in the middle of a long file costs what
//! the change costs, not what the file does.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

/// How far apart two texts may be (lines added plus lines removed, after the
/// shared start and end are off) before the diff stops looking and calls the
/// whole stretch changed. Bounds the time and the memory of one diff.
const MAX_EDITS: usize = 500;

/// What happened to a stretch of lines of the new text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HunkKind {
    Added,
    Modified,
    /// Lines of the old text were here, before the line the hunk starts at.
    Removed,
}

/// One change, in the numbering of the new text. A removal has no lines: it
/// is the empty range at the line that now follows the removed ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hunk {
    pub kind: HunkKind,
    pub lines: Range<usize>,
}

/// The hash of one line, without its line break.
pub(crate) fn hash_line(line: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    line.hash(&mut hasher);
    hasher.finish()
}

/// The hash of each line of `text`: split at `\n`, a `\r` before it dropped. A
/// text that ends with a line break has an empty last line, as a buffer has.
pub(crate) fn hash_lines(text: &str) -> Vec<u64> {
    text.split('\n')
        .map(|line| hash_line(line.strip_suffix('\r').unwrap_or(line)))
        .collect()
}

/// The changes that make `old` into `new`, in order of line.
pub(crate) fn diff(old: &[u64], new: &[u64]) -> Vec<Hunk> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let (old, new) = (&old[prefix..], &new[prefix..]);
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (old, new) = (&old[..old.len() - suffix], &new[..new.len() - suffix]);

    let blocks = changed_blocks(old, new).unwrap_or_else(|| {
        vec![Block {
            old: 0..old.len(),
            new: 0..new.len(),
        }]
    });
    blocks
        .into_iter()
        .flat_map(|block| hunks_of(block, prefix))
        .collect()
}

/// A stretch of `old` that is replaced by a stretch of `new` (either may be
/// empty).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    old: Range<usize>,
    new: Range<usize>,
}

/// The hunks of one block, `prefix` lines further down.
fn hunks_of(block: Block, prefix: usize) -> Vec<Hunk> {
    let (removed, added) = (block.old.len(), block.new.len());
    let start = block.new.start + prefix;
    let hunk = |kind, lines| Hunk { kind, lines };
    if removed == 0 && added == 0 {
        return Vec::new();
    }
    if added == 0 {
        return vec![hunk(HunkKind::Removed, start..start)];
    }
    if removed == 0 {
        return vec![hunk(HunkKind::Added, start..start + added)];
    }
    // The lines that took the place of removed ones are changed; the ones
    // beyond them are new.
    let changed = removed.min(added);
    let mut hunks = vec![hunk(HunkKind::Modified, start..start + changed)];
    if added > changed {
        hunks.push(hunk(HunkKind::Added, start + changed..start + added));
    }
    hunks
}

/// The stretches where `old` and `new` differ, or `None` when they are further
/// apart than [`MAX_EDITS`].
fn changed_blocks(old: &[u64], new: &[u64]) -> Option<Vec<Block>> {
    let (n, m) = (old.len(), new.len());
    if n == 0 || m == 0 {
        return Some(match n + m {
            0 => Vec::new(),
            _ => vec![Block {
                old: 0..n,
                new: 0..m,
            }],
        });
    }
    let (steps, trace) = shortest_edit(old, new)?;
    Some(blocks_from(&trace, steps, n, m))
}

/// The length of the shortest edit script, and for each round `d` the furthest
/// points of the round before (`v[k]` is how far along `old` the path on
/// diagonal `k` got), which is what walking the path back needs.
fn shortest_edit(old: &[u64], new: &[u64]) -> Option<(isize, Vec<Vec<isize>>)> {
    let (n, m) = (old.len() as isize, new.len() as isize);
    let max = (n + m).min(MAX_EDITS as isize);
    let at = |k: isize| (k + max) as usize;
    let mut v = vec![0isize; 2 * max as usize + 1];
    let mut trace = Vec::new();
    for d in 0..=max {
        trace.push(v[at(-d)..=at(d)].to_vec());
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            let mut y = x - k;
            while x < n && y < m && old[x as usize] == new[y as usize] {
                x += 1;
                y += 1;
            }
            v[at(k)] = x;
            if x >= n && y >= m {
                return Some((d, trace));
            }
            k += 2;
        }
    }
    None
}

/// Walks the path back from the end and collects the edits as blocks, in order.
fn blocks_from(trace: &[Vec<isize>], steps: isize, n: usize, m: usize) -> Vec<Block> {
    let (mut x, mut y) = (n as isize, m as isize);
    // Each edit as the point it starts at, and whether it removes a line of
    // `old` (else it adds one of `new`), latest first.
    let mut edits: Vec<(usize, usize, bool)> = Vec::new();
    for d in (1..=steps).rev() {
        let before = &trace[d as usize];
        let k = x - y;
        let at = |k: isize| before[(k + d) as usize];
        let previous = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let (px, py) = (at(previous), at(previous) - previous);
        edits.push((px as usize, py as usize, previous == k - 1));
        (x, y) = (px, py);
    }

    let mut blocks: Vec<Block> = Vec::new();
    for (x, y, removes) in edits.into_iter().rev() {
        // An edit that starts where the block before it ended continues it.
        let joins = blocks
            .last()
            .is_some_and(|block| block.old.end == x && block.new.end == y);
        if !joins {
            blocks.push(Block {
                old: x..x,
                new: y..y,
            });
        }
        let block = blocks.last_mut().expect("a block was just pushed");
        match removes {
            true => block.old.end += 1,
            false => block.new.end += 1,
        }
    }
    blocks
}
