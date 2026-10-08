# TODO — path to 0.1.0

See `design.md` for decisions. Milestones are ordered; each ends in something runnable.

## M0 — Foundation
- [x] Decide what to keep from the old code (git history) vs. delete; clean the working tree
- [x] New crate skeleton, module layout from design.md, `thiserror` error enums per module
- [x] CI: GitHub Actions matrix (Linux + Windows): build, `cargo test`, `clippy`, `fmt --check`
- [x] Tokio runtime, `Event` enum, single-owner `AppState`, input channel + bulk channel
- [x] Ratatui terminal setup/teardown with panic-safe restore
- [x] Dirty-flag render loop capped at ~60 fps
- [x] Headless test harness: feed `Event`s, assert state

## M1 — Text core
- [x] `Position` (line, grapheme) and conversions to byte/char (`unicode-segmentation`)
- [x] `Buffer` on `ropey`, `version`, line endings (LF/CRLF) detection
- [x] `Edit` struct and `Buffer::apply`
- [x] Transactions + linear undo/redo (stores inverse edits and selection state)
- [x] Typing-burst coalescing for normal-editor undo (runs of typing, backspace or delete form one step; moves, newline, selection edits and undo/redo break a run; no time-based break yet)
- [x] Selections (list of ranges, one used), `desired_column`
- [x] Property tests: apply + invert == identity, undo/redo, position round-trip
- [x] Property tests for grapheme movement over emoji/CJK/combining marks
- [x] File open/save (atomic write, encoding: UTF-8 only in 0.1.0, clear error otherwise)

## M2 — Action + keymap
- [x] `Action` enum with serializable args and a plugin hole (grows as features land)
- [x] `KeyChord`, crossterm normalization, string parsing
- [x] Kitty keyboard protocol probe (`terminal::init`) + legacy fallback keymap layer
- [x] Layered keymap (contexts, defaults -> legacy -> user file, unbind support)
- [x] Default keymap + fallbacks written; IDE prefix chord chosen (`ctrl+g`, bound in M6 with the terminal pane)
- [x] Key-sequence resolver with timeout (needed for vim and prefix chords)

## M3 — Editor UI
- [x] Editor view: gutter, scrolling, selection highlight, cursor, display-width/tab handling in the view layer only
- [x] Normal-IDE editing: typing, delete, line/document motion, shift-select, undo/redo, save
- [x] Word motion (ctrl/alt+arrows), delete word, select all
- [x] Copy/cut/paste: internal register (charwise or linewise) mirrored to the system clipboard; whole-line copy/cut with no selection; linewise paste goes above the cursor line; bracketed paste is a separate path
- [x] Auto-indent on Enter (copies the leading whitespace left of the cursor, verbatim), Tab/Shift+Tab with the indent style detected from the file (tabs, or 2-4 spaces; fallback 4 spaces), block indent/outdent of selected lines, page up/down (+ shift to select)
- [x] Mouse: click, drag, wheel (shift+wheel and horizontal wheel scroll sideways), shift+click extends, double click selects a word run, triple click the line; on by default, `alt+m` (`toggle_mouse`) switches it at runtime
- [ ] Mouse follow-ups: auto-scroll while dragging past the edge (needs a repeating timer), middle-click paste, right-click menu, click in the gutter selects the line, wheel goes to the pane under the pointer once there are several panes, `mouse` setting in `settings.json`, Shift+click may be taken by the terminal itself (kitty uses shift for its own selection) so consider alt+click for extending
- [x] Status bar (file, modified marker, line/column, messages)
- [x] Tabs and splits: documents are shared by id, every pane has its own tab strip, splits (right/down) show the same document in a new pane with shared buffer, shared undo and cursors that follow the other pane's edits; ctrl+n new file, ctrl+w close tab (asks before discarding the last view of a modified document, pane closes with its last tab; closing the very last tab leaves an empty editor area and the app keeps running), ctrl+pageup/pagedown switch tabs, ctrl+k right/down split, alt+h/j/k/l and ctrl+k o move focus, click focuses a pane or switches a tab, middle-click on a tab closes it (same unsaved-changes confirmation, any tab, focus stays), the wheel scrolls the pane under the pointer, several files on the command line open as tabs
- [x] Quit with unsaved changes asks first: a modal lists every modified document; `ctrl+s` saves and `ctrl+d` discards the selected one (up/down to choose), `s` saves all and quits, `d` discards all and quits, `esc` cancels; each entry has a rule under it; a file that cannot be saved (no path, I/O error) stays listed with the error and can still be discarded; the app quits once no document is left to decide on
- [ ] Tabs/panes follow-ups: open a path that is already open by switching to it, overflow markers in a crowded tab bar, close other/all tabs, move a tab between panes, resize panes (drag the divider), close pane without closing its tabs one by one, reopen closed tab, save all, per-pane tab history
- [x] Clipboard (arboard, OSC 52 write-only fallback, one-time notice when the system clipboard is unreadable)
- [x] Per-component frames in the layout file: border type, title text, title alignment and title/border theme slots (`Framed(component: X, frame: (...))`)
- [x] `[background]` in theme.toml: color + opacity, blended with the terminal background (OSC 11 query) or transparent so the terminal's own opacity/blur shows
- [ ] Notifications overlay
- [x] Command palette (`f1`, `alt+p`; not `ctrl+shift+p`, which kitty and Windows Terminal keep for themselves): fuzzy search over hand-written action titles ("File: Save"), actions with an argument expanded per value ("Pane: Focus Left"), the bound keys shown beside each entry, enter runs it, esc closes; every action that has a title is listed explicitly in `Action::palette_actions`
- [ ] Palette follow-ups: argument prompts (open file path, go to line), save as for untitled files, recently used first, mouse selection, paste into the query

## M4 — Explorer + search
- [ ] Project tree, lazy load, gitignore-aware
- [ ] File watcher events -> tree refresh and external-change prompts for open buffers
- [ ] Create / rename / delete / move
- [ ] Fuzzy file finder
- [ ] Project-wide text search with results list

## M5 — Syntax highlighting
- [ ] tree-sitter integration, grammar bundle (starter languages)
- [ ] Parse worker with `version` stale-result discard
- [ ] Incremental reparse from `Edit`
- [ ] Smart indent: add a level after an opening token (`{`, `(`, `[`, `:`) and dedent on a closing one, using the syntax tree to skip strings and comments (Enter copies indentation verbatim until then)
- [ ] Highlight theme as data; default dark + light themes

## M6 — Integrated terminal
- [ ] `portable-pty` spawn (Unix + ConPTY), shell selection
- [ ] `alacritty_terminal` grid, render into Ratatui
- [ ] Batched PTY output events, bounded channel, backpressure test (`cat` huge file keeps input responsive)
- [ ] Resize, scrollback, alt screen, bracketed paste, mouse reporting passthrough
- [ ] Multiple terminal tabs
- [ ] Focus handling: all keys to shell except IDE prefix chord

## M7 — Vim layer
- [ ] Mode state machine: normal / insert / visual / visual-line
- [ ] Motions, operators, text objects, counts
- [ ] Registers, `.` repeat, `/` search + `n`/`N`
- [ ] Minimal ex commands: `:w :q :wq :e`
- [ ] Config toggle for vim on/off; mode indicator in the status bar
- [ ] Vim parser tests (table-driven: keys in, edits out)

## M8 — Config + polish
- [ ] `settings.json` (Zed-style: JSON with comments, user file layered over defaults; project override file optional). Settings: theme, shell, vim on/off, indent fallback width/style and an option to turn off indent detection from the file, tab display width, sequence timeout. Needs a JSONC parser (or comment stripping) and typed errors shown as notifications. Keymap stays in `keymap.toml` unless we decide to move it.
- [ ] Mouse: pane focus by click (needs several panes)
- [ ] Error handling pass: every recoverable error surfaces as a notification; no `unwrap` outside tests
- [ ] Performance pass: large file open (10 MB), long lines, fast scroll
- [ ] Manual test pass on Windows Terminal, one kitty-capable and one non-kitty terminal on Linux

## M9 — Release 0.1.0
- [ ] README (install, keybinds, config), license
- [ ] Release CI: binaries for Linux and Windows (macOS best-effort), checksums
- [ ] Known-limits section (long single-line files, UTF-8 only, no undo tree, no LSP/debugger)
- [ ] Tag `v0.1.0`

## Post-0.1.0 backlog
- [ ] LSP client (possibly as a Lua plugin)
- [ ] Lua plugin runtime on top of the Action registry
- [ ] Debugger via DAP (e.g. Rust through `lldb-dap`), possibly as a Lua plugin
- [ ] Multi-cursor, undo tree, visual-block, vim macros, git integration

## Open questions (resolve before the milestone that needs them)
- [ ] Clipboard strategy details (M3)
- [ ] Project search implementation: crate vs `rg` (M4)
- [ ] Mouse scope (M8)
