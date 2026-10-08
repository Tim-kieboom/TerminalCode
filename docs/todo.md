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
- [ ] Auto-indent, page up/down
- [ ] Mouse: click to place cursor, drag to select, scroll wheel
- [x] Status bar (file, modified marker, line/column, messages)
- [ ] Tabs and splits (multiple open documents)
- [x] Clipboard (arboard, OSC 52 write-only fallback, one-time notice when the system clipboard is unreadable)
- [ ] Notifications overlay
- [ ] Command palette over the Action registry

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
- [ ] `config.toml` (theme, shell, vim on/off, tab width, etc.)
- [ ] Mouse: click to place cursor, scroll, pane focus
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
