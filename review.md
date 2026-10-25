# TerminalCode: full code review

Reviewed against `docs/Guideline PROMPT REVIEW.md` and `docs/Guideline PROMPT CODE.md`. The review covers about 40k lines of non-test Rust, plus a sample of the tests, in the editor package and 13 workspace crates (544 `.rs` files, 138k lines in all).

**Method.** The codebase was reviewed in five parts, one per area, and each part is a section below. The findings come from reading the code, so most are not confirmed by a failing test. `cargo fmt --all --check` is clean. `cargo clippy --workspace --all-targets` reports 14 warnings, listed in the next section. Part E's reviewer could not run `cargo check` (a glibc error in the sandbox), so its dead-code items come from reading the code and from the clippy warnings.

## Summary

| Part | Scope | Critical | Major | Minor | Suggestion |
|---|---|---|---|---|---|
| A | `crates/lsp`, `dap`, `pty` | 0 | 2 | 24 | 7 |
| B | `crates/ids`, `buffer`, `files`, `clipboard`, `jumplist`, `launch`, `breakpoints`, `syntax`, `ui`, `plugin-api` | 0 | 1 | 26 | 10 |
| C | `src/app` | 0 | 6 | 30 | 8 |
| D | `src/components` | 2 | 5 | 32 | 5 |
| E | the rest of `src` (plugins, git, terminal, event, settings, config, keymap, persist) and the test suites | 0 | 3 | 32 | 8 |
| **Total** | | **2** | **17** | **144** | **38** |

**Overall.** The code follows the guidelines well. Non-test code has almost no `unwrap`, `expect` or `panic!`. Every `unsafe` block has a SAFETY comment, though one is wrong (see E-M2). Errors are mostly concrete `thiserror` types, and early returns are used consistently. The weaknesses are structural:

- very large functions;
- copy-pasted logic;
- stringly or boolean-typed APIs;
- file layout that does not follow the new globals, then `pub`, then private rule;
- about 1,620 `unwrap()`s in tests.

## Fix first

These are the most concrete problems, ordered by likely user impact.

1. **D-C1, completion box highlight.** The box reads a stale scroll offset, so no row is highlighted whenever the list scrolls (`src/components/completion_box.rs:183`).
2. **D-C2, debug-hover selection.** `move_by` never scrolls the selection into view, so with more than 14 rows the selection moves off-screen (`src/components/debug/hover.rs`).
3. **C-1, keymap corruption.** A chord written to `keymap.toml` is not escaped (`plugin_keys.rs:521`). A chord such as `alt+\` or `alt+"` produces invalid TOML.
4. **E-M3, lost saved state.** A corrupt `state.json` is overwritten by the next save, which loses breakpoints, watches and trust decisions (`src/persist/saved_state.rs:326`).
5. **A-M1, duplicated edits.** `parse_workspace_edit` applies both `changes` and `documentChanges`. A server that sends both gets every edit duplicated (`crates/lsp/src/features/rename.rs:415-459`).
6. **A-M2, blocking writes.** LSP and DAP writes run on the caller's thread under the shared writer lock. A server that stops reading stdin can block the app thread and can deadlock the reader thread. This is inferred from the code, not reproduced.
7. **E-M1, unbounded plugin memory.** A plugin can queue unbounded host memory through `host.send` (`src/plugins/instance.rs:114`).
8. **E-M2, process-group kill.** The kill can hit a PID that was already reaped, and the SAFETY comment is wrong on that path (`src/plugins/exec.rs:178-210`).
9. **B-M1, `RunStream` has no `Unknown` variant.** A new stream kind from a newer host would be dropped by an older plugin, which breaks the crate's additive-only promise (`crates/plugin-api/src/message.rs:605`).

## Clippy warnings

All fourteen are dead-code or style warnings. None is a correctness issue.

- Never read or never used: `RootPrompt::document` (`root_prompt.rs:49`, `:68`), `definition_list.rs:91` (`entries`, `selected`), `Event::SetPluginView` (`event/mod.rs:14`), `ActionError::Busy` (`git/actions.rs:24`), `Reply::Status` (`plugins/install/tests.rs:19`).
- Style: `hover.rs:512-515` (a unit value passed to a function, 4 times), `plugins.rs:583` (collapsible `if`), `clone()` on a `Copy` `Selection` in two tests (`call_brackets_and_docs.rs:12`, `real_rust_analyzer.rs:73`).

## Cross-cutting themes

- **Duplication.**
  - The lsp and dap crates share byte-identical framing and near-identical connection code.
  - Parse helpers are repeated across the LSP feature modules.
  - The debugger tree views and the popup lists are repeated.
  - Selection and scroll navigation is reimplemented at least six times.
  - The test helpers `wait_until` and `git()` are copied across many files.
- **Stringly and boolean types.** Examples are `Result<_, String>` errors, `bool` flags and anonymous tuples in public APIs.
- **File layout.** Private items and constants sit among or before the public ones in many files.
- **Large functions.** `handle_mouse` is 287 lines, `debug_answers` is 129, `draw_lines` is 144, and `handle_key` is 123.
- **Tests.** About 1,620 `unwrap()`s, wall-clock sleeps that can flake, a test that depends on the host's git config, and inconsistent test-file naming across crates. No test writes into the repo.

## Dead code

- `Event::SetPluginView`, `ActionError::Busy` and `IdeError::Unknown` are never constructed.
- The stale `#[allow(dead_code)]` on `mod plugins` (`src/lib.rs:38`) and on `Settings::language_servers` may hide more.

---

The detailed findings follow, one part per area. Each finding has a severity, a location, the guideline violated, an explanation and a suggested improvement.


---

<!-- Part A -->
# Review A: crates/lsp, crates/dap, crates/pty

## Overall assessment

- **trcode-pty**: Small and well built. The bounded writer queue, the reader slice and the lock discipline are clear and documented. There is no `unsafe`, and no `unwrap`/`expect`/`panic` in non-test code. Findings are API polish (`Written` is unnameable, `PtyError` carries strings) and some duplication.
- **trcode-dap**: Clean protocol layer with good module docs and no panics in production code. It is a near copy of the lsp connection and framing code (a documented choice). The notable issues are silently dropped responses, hidden serialization errors, and a needless `body.clone()` per event.
- **trcode-lsp**: Large and mostly readable. Error handling is concrete (`LspError`, `ConfigError`). The main problems are one correctness risk (`WorkspaceEdit` merging `changes` and `documentChanges`), blocking writes to the server from the UI thread, and a lot of copy-pasted parsing helpers. `Manager` is a 40-method forwarding facade split over 11 `impl` blocks.
- Cross-cutting: no `unsafe` in any of the three crates. Non-test code has no `unwrap`/`expect`/`panic!`. File layout (globals, then pub, then private) is often not followed, because private helpers and types sit between public items.

---

## Major

### M1. `WorkspaceEdit` applies `changes` and `documentChanges` together, so a server that sends both gets every edit twice
- **Severity**: Major
- **Location**: `crates/lsp/src/features/rename.rs:415-459` (`parse_workspace_edit`); the capability is advertised at `crates/lsp/src/protocol/handshake.rs:399-402`
- **Guideline**: Correctness (bugs, invalid assumptions)
- **Explanation**: The client declares `workspaceEdit.documentChanges: true`. The LSP spec says that a client that handles `documentChanges` must prefer it over `changes` when both are present. The code reads `changes` first, then `documentChanges`, and `add_edits` extends the same per-URI list. A server that sends both (several do, for compatibility) produces duplicated edits for each file. Applying those to the buffer corrupts the text, for rename, code actions and `workspace/applyEdit`. rust-analyzer sends only `documentChanges`, so this is latent. The tests (`rename_tests.rs`) never cover a message with both keys.
- **Suggested improvement**: When `documentChanges` is an array, parse only it and ignore `changes`. Fall back to `changes` otherwise. Add a test with both keys present.

### M2. Writes to the server run synchronously on the caller's thread and hold the shared writer lock
- **Severity**: Major
- **Location**: `crates/lsp/src/protocol/connection.rs:145-170` and `:262` (the reader thread also writes under the same lock); `crates/dap/src/connection.rs:442-456`
- **Guideline**: Correctness / architecture (the supervisor docs state "a broken server never blocks typing")
- **Explanation**: `Connection::notify` and `request` call `write_message`, which does `write_all` and `flush` on the server's stdin pipe under `Mutex<Option<Box<dyn Write>>>`. If the server stops draining stdin (hung, busy, or SIGSTOP), the first message larger than the pipe buffer (about 64 KB, for example a `didOpen` of a big file) blocks the app thread. This is the problem `trcode-pty` solves with `BoundedQueue` plus a writer thread. The reader thread also answers server requests through the same lock (`write_to` at `connection.rs:262`). If the app thread is blocked in a write while holding the lock, the reader cannot reply and stalls too. That is a lock-and-pipe deadlock risk if the server in turn waits on its stdout.
- **Suggested improvement**: Give the connection a writer thread fed by a channel or `BoundedQueue`, as pty does. The app thread then enqueues and never blocks, and the reader thread no longer needs the write lock. Fail with `Closed` when the queue is full or closed.

---

## Minor

### m1. `Written` is returned by a public method but not re-exported
- **Location**: `crates/pty/src/lib.rs:5-6`; `crates/pty/src/session.rs:39` and `:249`
- **Guideline**: API design (public implementation details, poor encapsulation)
- **Explanation**: `Session::write` returns `Written`, a `pub` struct in the private `session` module. Callers cannot name the type (for example in a signature or a helper). `complete: bool` is also redundant, since it equals `sent == bytes.len()`.
- **Suggested improvement**: Add `Written` to the `pub use session::{...}` list. Consider replacing the bool with a `Result<(), Partial { sent }>`, or an enum `Written::{All, Partial(usize)}`.

### m2. `Shared` is constructed twice with identical field lists
- **Location**: `crates/pty/src/session.rs:178-185` and `:215-222`
- **Guideline**: Readability / maintainability (duplication)
- **Explanation**: Both `spawn` and `detached` build all six fields by hand, so adding a field means editing two places. `spawn` is also about 75 lines and mixes pty setup, thread spawning and struct building.
- **Suggested improvement**: Add `Shared::new(rows, columns)`. Split the thread startup into a helper.

### m3. `PtyError` stores only strings and loses the source error
- **Location**: `crates/pty/src/error.rs:4-8`
- **Guideline**: Error handling (generic String errors; suggest concrete types)
- **Explanation**: `Open(String)` and `Spawn { message: String }` come from `error.to_string()` at `session.rs:152`, `:163`, `:168` and `:172`. The original `anyhow::Error` from `portable_pty` is dropped, so there is no `source()` chain and callers cannot match on the cause.
- **Suggested improvement**: Store the error with `#[source]` (`Box<dyn Error + Send + Sync>`, or `anyhow::Error` wrapped).

### m4. Test-only helpers live in production code
- **Location**: `crates/pty/src/shell.rs:41-47` (`Shell::script`, hard-coded `"sh"`, doc says "for tests"); `crates/lsp/src/session/progress.rs:301-304` (`#[cfg(test)] pub fn is_idle`)
- **Guideline**: API design (public surface), Testing (helpers kept in test files)
- **Explanation**: `Shell::script` is a public API of the shipped crate that only tests use and that is wrong on Windows. `is_idle` pollutes the impl block with test code.
- **Suggested improvement**: Move both into the tests' own support module (`pty/src/tests/support.rs`), or gate `script` with `#[cfg(test)]`.

### m5. `Session::finished` sleeps on the caller's thread
- **Location**: `crates/pty/src/session.rs:363-378`
- **Guideline**: Performance / readability (blocking in a method documented for the UI loop)
- **Explanation**: It polls `exit_code` up to 40 times with `thread::sleep(5 ms)`, so it can block the calling thread for 200 ms. The caller is presumably the draw/event thread, which this crate otherwise guards carefully. The sleeping is a workaround for the reader and the reaper racing.
- **Suggested improvement**: Return `None` and let the next wake-up retry. Or have the watcher thread (already running) record the exit code once `try_wait` succeeds and expose it without sleeping.

### m6. Timing-dependent assertion in a unit test
- **Location**: `crates/pty/src/tests/queue_tests.rs:47`
- **Guideline**: Testing (not brittle)
- **Explanation**: `assert!(start.elapsed() < Duration::from_millis(15))` fails on a loaded CI runner. The `Err(PushError::Full(2))` assertion above it already proves the behavior. `flood_tests.rs` explicitly avoids this pattern.
- **Suggested improvement**: Remove the elapsed-time assertion.

### m7. `serialize` failure is hidden when answering `runInTerminal`
- **Location**: `crates/dap/src/client.rs:221-223`
- **Guideline**: Error handling (hidden errors)
- **Explanation**: `serde_json::to_value(response).unwrap_or(Value::Null)` turns a serialization failure into a successful answer with a null body. The adapter then believes the program started with no process id.
- **Suggested improvement**: Map the error to `Rejection::new(...)` or `DapError::Unexpected`. `RunInTerminalResponse` cannot fail today, but the hidden swallow is not a pattern to keep.

### m8. A response that does not parse is dropped silently, and its request times out
- **Location**: `crates/dap/src/connection.rs:536-540`; `crates/dap/src/message.rs:269`
- **Guideline**: Error handling (hidden errors) / correctness
- **Explanation**: `Message::from_value` requires `command` on a response, but the spec only needs `request_seq`, `success` and `seq`. An adapter that omits `command` on a response makes `from_value` fail. The reader then does `continue`, nothing is delivered, and the caller waits the full timeout and sees `Timeout` rather than the real problem. There is no log either.
- **Suggested improvement**: Make `command` optional for responses (default to an empty string), or deliver a `DapError::Unexpected` to the waiting request when a message has a readable `request_seq`.

### m9. Stringly-typed error and state values
- **Location**: `crates/dap/src/error.rs:11-18` (`Closed(String)`, `Unexpected { command, detail: String }`); `crates/dap/src/client.rs:41` (`Update::Closed(String)`) and `:219` (`Result<RunInTerminalResponse, String>`); `crates/lsp/src/protocol/error.rs:9-20`; `crates/lsp/src/session/launch.rs:380` (`StartError::Spawn(String)`); `crates/lsp/src/session/config.rs:158` (`Entry::into_config -> Result<_, String>`, and `ConfigError::Entry { problem: String }`)
- **Guideline**: Error handling (generic String errors), Types (strong types)
- **Explanation**: Reasons are formatted early, so the `io::Error` source is lost (`StartError::Spawn` has no `source`). `StartError` also implements neither `Error` nor `Display`; it offers a `reason()` that allocates. `Closed(String)` cannot be matched by cause (exit versus broken pipe versus bad frame).
- **Suggested improvement**: Use `Spawn { command, #[source] io::Error }`, derive `thiserror::Error` for `StartError`, and give `Closed` an enum reason (`Eof`, `Frame(FrameError)`, `Write(io::Error)`). Give `into_config` a small error enum.

### m10. `Event::parse` clones every event body
- **Location**: `crates/dap/src/types.rs:121-123` (`from`), used at `:103-111`
- **Guideline**: Ownership (unnecessary clones/allocations)
- **Explanation**: `serde_json::from_value(body.clone())` deep-clones the body for `stopped`, `output`, `thread` and `breakpoint`. `output` events from a chatty debuggee can be large and frequent.
- **Suggested improvement**: Use `T::deserialize(&body).ok()` (`&Value` implements `Deserializer`). That borrows, and `body` is still available for the `Other` fallback.

### m11. Timeout message reports `0 s` for sub-second timeouts
- **Location**: `crates/dap/src/connection.rs:473-475`; `crates/lsp/src/protocol/connection.rs:179-182`
- **Guideline**: Correctness (small), types
- **Explanation**: `seconds: timeout.as_secs()` truncates, so a 500 ms timeout reads "did not answer ... within 0 s".
- **Suggested improvement**: Store the `Duration` and format it with `{:?}`, or use `as_secs_f32`.

### m12. Public surface of `trcode-dap` is wider than needed
- **Location**: `crates/dap/src/lib.rs:20-23`
- **Guideline**: API design (excessive visibility, public implementation details)
- **Explanation**: `pub use types::*` hides the real surface and exports generic names (`Thread`, `Event`, `Output`, `Source`, `Nothing`) with the glob. `framing` (`read_message`, `write_message`, `FrameError`, `MAX_BODY`) is public, but the only external consumer is the in-crate `test_support`, which can reach `crate::framing`.
- **Suggested improvement**: List the re-exports explicitly. Make `framing` crate-private (the lsp crate already does this).

### m13. Dap test files do not follow the `{name}_tests.rs` convention
- **Location**: `crates/dap/src/tests/{client,connection,framing,message,real,session,spawn}.rs`, `crates/dap/src/tests.rs`; also `crates/lsp/src/session/cargo/tests.rs` and `crates/pty/src/replies/tests.rs`
- **Guideline**: Testing / file layout (`{name}_tests.rs` next to the source)
- **Explanation**: lsp and pty use `tests/foo_tests.rs`, dap uses `tests/foo.rs`. Three conventions across three sibling crates make test discovery inconsistent.
- **Suggested improvement**: Rename dap's test files to `*_tests.rs` (and settle on one layout for the workspace).

### m14. `LspPosition` parsing and serialization are copy-pasted
- **Location**:
  - Parsing: `features/completion.rs:345-350`, `features/definition.rs:179-184`, `features/diagnostics.rs:108-113`, `features/rename.rs:512-517`
  - JSON building: `code_action.rs:156`, `formatting.rs:337`, `sync.rs:285,299`, `completion.rs:250`, `definition.rs:130`, `hover.rs:225`, `rename.rs:355,369`
- **Guideline**: Readability / maintainability (duplication; do not invent custom APIs, use `From`/`TryFrom`)
- **Explanation**: Four identical `position()` functions and about nine inline `json!({"line":..,"character":..})` literals. A change to the encoding must be made 13 times.
- **Suggested improvement**: Add `LspPosition::from_json(&Value) -> Option<Self>` and `LspPosition::to_json(self) -> Value` (or `From<LspPosition> for Value` and `TryFrom<&Value>`) in `protocol/position.rs`, plus a `range_json` helper.

### m15. The capability check `offered()` is repeated six times
- **Location**: `features/code_action.rs:55`, `definition.rs:102`, `hover.rs:197`, `formatting.rs:281` and `:290`, `rename.rs:307`
- **Guideline**: Maintainability (duplication)
- **Explanation**: Each is the same `match caps.get(key) { Some(Bool(b)) => *b, Some(Object(_)) => true, _ => false }`, differing only in the key.
- **Suggested improvement**: One `fn provider_offered(capabilities: &Value, key: &str) -> bool` in a shared module.

### m16. Duplicate edit types, and error text that names the wrong method
- **Location**: `features/completion.rs:185-189` (`TextEdit`) versus `features/rename.rs:262-266` (`RenameEdit`), identical fields. `formatting.rs:304,349` returns `RenameEdit`. `rename.rs:412-455` and `:473` hard-code the `RENAME` method name in `Unexpected` errors from `parse_workspace_edit`/`add_edits`, which is also called from `code_action.rs:199` and `manager.rs:129`.
- **Guideline**: Types (named, meaningful structs), error handling (useful errors)
- **Explanation**: Two structs for the same LSP `TextEdit`. A formatting answer is typed as a "rename edit". A malformed `workspace/applyEdit` or code-action edit is reported as a malformed `textDocument/rename`.
- **Suggested improvement**: Use one `TextEdit` type everywhere. Pass the method name into `parse_workspace_edit` (as `parse_text_edits` already does).

### m17. Stale module doc contradicts the declared capabilities
- **Location**: `crates/lsp/src/features/completion.rs:6`
- **Guideline**: Readability (docs must match code)
- **Explanation**: The doc says "The client declares no snippet support, so a server sends plain text". `handshake.rs:357` sets `"snippetSupport": true`, and `snippet.rs` exists to expand them.
- **Suggested improvement**: Update the doc.

### m18. Markdown stripping corrupts code in hover and completion docs
- **Location**: `crates/lsp/src/features/docs.rs:38-63` (`strip_inline`), `:67-80` (`link_at`), applied by `hover.rs:256` and `completion.rs:316`
- **Guideline**: Correctness
- **Explanation**: Code-fence marker lines are dropped, but the lines inside the fence still go through `strip_inline`. `'*' | '_'` followed by the same character is removed, so `__init__`, `__rust_alloc` or `a ** b` lose characters. A `[x][y]` or `[x](y)` pattern is collapsed to its text, so `matrix[i][j]` in a code sample becomes `matrixi`. Hover signatures are exactly where this appears.
- **Suggested improvement**: Track an `in_fence` flag in `plain_text` and push fenced lines through unchanged. Only strip `**`/`__` when it is a delimiter at a word boundary.

### m19. Unbounded recursion on server-controlled snippet text
- **Location**: `crates/lsp/src/features/snippet.rs:404-506` (`emit` -> `dollar` -> `braced` -> `emit`)
- **Guideline**: Correctness / robustness
- **Explanation**: Each `${1:` nests another recursion level, with one Rust stack frame set per 4 characters of input. A server (or a huge generated snippet) with tens of thousands of nested placeholders overflows the stack, which aborts the whole editor.
- **Suggested improvement**: Thread a depth counter through `emit`/`braced` and treat text beyond a limit (say 32) literally.

### m20. Weak types: booleans and tuples in public APIs
- **Location**: `crates/lsp/src/session/supervisor.rs:28-32` (`Notice { error: bool }`); `crates/lsp/src/session/manager.rs:218-224` (`respond_apply_edit(applied: bool, reason: Option<String>)`, where `reason` is only meaningful when `applied` is false); `features/formatting.rs:312-333` (`insert_spaces: bool` in 2 of the requests, the range one having 6 parameters); `features/diagnostics.rs:234` (`counts -> (usize, usize, usize)`); `features/rename.rs:301` (`range: Option<(LspPosition, LspPosition)>`)
- **Guideline**: Types (enums over booleans, named structs)
- **Explanation**: Call sites like `respond_apply_edit(key, id, false, None)` read poorly, and `(true, Some(reason))` is an invalid state that is representable.
- **Suggested improvement**: `enum NoticeLevel { Info, Error }`, `Result<(), String>` or `enum ApplyOutcome { Applied, Refused(String) }`, `struct FormatOptions { tab_size, insert_spaces }`, and a `DiagnosticCounts { errors, warnings, other }` struct.

### m21. `Session` exposes mutable public fields next to a constructor
- **Location**: `crates/lsp/src/session/launch.rs:350-374`
- **Guideline**: Types ("Avoid public mutable fields"), API design (poor constructors)
- **Explanation**: `connection`, `incoming` and `server` are `pub`, and `Session::new` also exists. Fields can be swapped independently (a `Server` from one session with the `Connection` of another). The private `guard: Box<dyn Send>` is only held for its `Drop`, which `dyn` hides; a concrete `Option<ProcessGuard>` would do for the real launcher.
- **Suggested improvement**: Make the fields private with accessors (or `pub(crate)`), and keep `new`.

### m22. `Manager` is a 40-method forwarding facade spread over 11 `impl` blocks, and it swallows request errors
- **Location**: `crates/lsp/src/session/manager.rs:55-490`; `supervisor.rs:74-203` (two blocks)
- **Guideline**: API design (excessive surface), Error handling (hidden errors), Readability
- **Explanation**: Every `request_*` is `let session = self.sessions.get(key)?; feature::request(&session.connection, ...).ok()`. The `.ok()` discards the `LspError`, so "server not running" and "write failed / connection closed" are indistinguishable, and the failure reason is never shown to the user. Most paths use fully qualified `crate::...` instead of imports. The 11 separate `impl Manager` blocks (`:191, :209, :245, :253, ...`) have no organizing principle beyond feature grouping.
- **Suggested improvement**: Merge the `impl` blocks. Add `fn session(&self, key) -> Option<&Session>` and let the feature code take `&Session`. Return `Result<_, RequestError>` (`NotRunning`, `Lsp(LspError)`) or at least log the swallowed error.

### m23. `DocumentSync::update` is long and deeply nested
- **Location**: `crates/lsp/src/session/sync.rs:108-166`
- **Guideline**: Style (function size, nested control flow, early returns)
- **Explanation**: About 58 lines with four nesting levels inside the per-document loop. Three separate branches push `doc.id` to `resets` for reloads, and the `retain` closure scans `docs` linearly per open document (O(open times docs)).
- **Suggested improvement**: Extract `fn sync_document(told, doc, output)` and `fn close_gone(server, docs, output)`. Use a `HashMap<DocumentId, &DocSnapshot>` for the lookup.

### m24. File layout: private items placed among or before the public ones
- **Location** (examples):
  - `crates/lsp/src/protocol/connection.rs:35-48` (private aliases and `Pending` between `Incoming` and `Connection`)
  - `crates/dap/src/connection.rs:332-344`
  - `crates/dap/src/types.rs:121-123` (`from`) and `:146` (`yes`) between pub types
  - `crates/lsp/src/features/diagnostics.rs:88-113`
  - `crates/lsp/src/features/completion.rs:286-350` (private parse helpers before the pub `PendingResolve`)
  - `crates/lsp/src/features/completion_state.rs:117,179,188,202` (private methods between pub ones)
  - `crates/lsp/src/session/progress.rs:226` (`Job` before `Progress`)
  - `crates/lsp/src/session/sync.rs:63-88` and `:303` (pub `language_id` after private functions)
  - `crates/pty/src/session.rs:68,132` (private `Shared`/`Message` before pub impls) and `:483-488` (constants mid-file)
  - `crates/pty/src/mouse.rs:73-75` (`MOTION`/`WHEEL` constants after the function that uses them)
- **Guideline**: Style (globals, then pub items, then private items)
- **Explanation**: A reader meets private implementation detail and mid-file constants before the public surface. Exceptions where the guideline is followed: `framing.rs`, `message.rs` and `position.rs` in both protocol crates.
- **Suggested improvement**: Move constants to the top of each file, pub items next, and private helpers and types at the bottom.

---

## Suggestions

### s1. Framing, message types, connection and test pipes are duplicated between lsp and dap
- **Location**: `crates/lsp/src/protocol/{framing,connection}.rs` versus `crates/dap/src/{framing,connection}.rs`; `crates/lsp/src/test_support.rs:20-66` versus `crates/dap/src/test_support.rs:20-66`
- **Guideline**: Maintainability (duplication)
- **Explanation**: `framing.rs` is byte-identical apart from comments. `connection.rs` (pending table, `Reading` thread, `PendingReply` drop) differs only in the id type and the reply type. The `PipeReader`/`PipeWriter` code in `test_support` is identical. The dap module doc says this is deliberate ("so that the two crates do not depend on each other"), but any fix (M2, m8, m11) has to be made twice.
- **Suggested improvement**: Extract a leaf crate (for example `trcode-rpc`) with `read_message`/`write_message`, the `Pending` table and the pipe test helpers. Both clients depend on it, still without depending on each other.

### s2. An unparsable `runInTerminal` is answered with the message "unsupported"
- **Location**: `crates/dap/src/client.rs:273-285`
- **Guideline**: Error handling (useful errors)
- **Explanation**: If `runInTerminal` arrives with arguments that fail to deserialize, the adapter is told `unsupported`, which is false and misleading.
- **Suggested improvement**: Answer `Rejection::new("invalid arguments")` for that case, and reserve `unsupported` for other commands.

### s3. `find_program_in` uses a nested `flat_map`/`map(move ...)` chain
- **Location**: `crates/dap/src/client.rs:372-381`
- **Guideline**: Readability (difficult iterator chains)
- **Suggested improvement**: Write it as two plain `for` loops that return on the first existing file.

### s4. Closure `cargo` shadows the imported module `cargo`
- **Location**: `crates/lsp/src/session/config.rs:234-236`
- **Guideline**: Readability (poor naming)
- **Explanation**: `let cargo = |dir| ...` followed by `cargo::includes(...)` compiles (separate namespaces) but reads as a bug.
- **Suggested improvement**: Rename the closure `has_manifest`.

### s5. Every completion item keeps a full clone of its JSON
- **Location**: `crates/lsp/src/features/completion.rs:318` (`raw: value.clone()`); `parse_completion(&Value)` at `:259`, called from `:231,237`
- **Guideline**: Ownership / performance (unnecessary clones, to be measured first)
- **Explanation**: rust-analyzer returns thousands of items with documentation. The whole list is cloned in `parse_item`, while the original `Value` is dropped right after. The raw value is only needed when the server supports `completionItem/resolve`.
- **Suggested improvement**: Take the answer `Value` by value and move the item out of it, or store `raw` only when `Provider::resolve` is true. Measure before changing.

### s6. Per-poll and per-call allocations and syscalls
- **Location**: `crates/lsp/src/session/manager.rs:99` (all `ServerKey`s, each with a `String` and a `PathBuf`, cloned on every `poll`); `crates/lsp/src/session/language.rs:51-52` (two `canonicalize` calls per `.json` file)
- **Guideline**: Performance (only after measuring)
- **Suggested improvement**: Iterate the sessions without cloning keys (collect only the keys that have something waiting), and cache the canonical `config_dir` settings path.

### s7. `trcode-lsp` re-exports about 100 names flat at the crate root
- **Location**: `crates/lsp/src/lib.rs:9-37`
- **Guideline**: API design (public surface)
- **Explanation**: Parse functions (`parse_hover`, `parse_completion`, ...), `Pending*` types and feature structs share one namespace. Some, such as `client_capabilities`, are only reachable through a link in the docs.
- **Suggested improvement**: Expose the feature modules (`lsp::hover`, `lsp::rename`) and keep only the core types at the root.

---

<!-- Part B -->
# Review B: ids, buffer, files, clipboard, jumplist, launch, breakpoints, syntax, ui, plugin-api

Reviewed against `docs/Guideline PROMPT REVIEW.md` and `docs/Guideline PROMPT CODE.md`. All paths are relative to `crates/`. Line numbers were checked with grep against the current tree.

## Assessment per crate

- **ids**: Tiny and clean. No violations.
- **buffer**: Solid design with concrete error types, a transactional edit API and good docs. Weaknesses: three boolean lock flags, undo/redo that is not atomic on error, and file-layout drift.
- **files**: Good atomic-write and sudo design. Findings: a `String` error type in `removal`, an app-specific error enum, a test-only accessor in the public API, a wiping gap in `Secret`, and no crate-local tests.
- **clipboard**: Small and clean. No significant violations.
- **jumplist**: Correct and compact. One `&PathBuf` parameter. The byte-shift logic is duplicated (see the cross-crate finding).
- **launch**: Well documented and tested. Findings: boolean parameters and fields, `RunList::build` is long, and one test depends on the repo layout.
- **breakpoints**: Clear logic with a good doc header. The shift logic is duplicated, `Answer` uses a boolean plus a message, and private types sit before public ones.
- **syntax**: Good worker and coalescing design. Findings: a garbled doc comment, an unchecked subtraction, an in-flight-failure race in `DocumentSyntax`, a `String` failure type, no panic recovery in the worker, and stray `{ }` blocks.
- **ui**: Good layout validation. Findings: unguarded arithmetic in `bar.rs`, `Option<Option<Color>>`, boolean parameters, the file layout of `layout.rs` and `theme.rs`, and an error message that contradicts the validation.
- **plugin-api**: Careful wire design. `RunStream` has no `Unknown`, which breaks the crate's own compatibility contract. The manifest is stringly typed and `parse_all` has an awkward signature.

Findings by severity: Critical 0, Major 1, Minor 26, Suggestion 10.

---

## Major

### M1. `RunStream` has no `Unknown` catch-all, which breaks the crate's additive-only ABI promise
- **Severity**: Major
- **Location**: `plugin-api/src/message.rs:605` (`RunStream`); promise at `plugin-api/src/lib.rs:6-13`. `Thread` at `plugin-api/src/manifest.rs:33` is similar.
- **Guideline**: API Design and Correctness (invalid assumptions).
- **Explanation**: The crate doc says every enum has an `Unknown` catch-all and that new kinds are only ever added. `RunStream` is `#[non_exhaustive]` but has only `Stdout` and `Stderr`. If a later host adds a stream kind, `ToPlugin::RunOutput` fails to deserialize in an older plugin. The wasm `guest::deliver` (`guest.rs:71`) then drops the whole message silently, so output is lost instead of degrading. `Thread` has the same gap, so an unknown `thread = "..."` rejects the whole manifest.
- **Suggested improvement**: Add `#[serde(other)] Unknown` to `RunStream`. Decide deliberately whether `Thread` should accept an unknown value, and document the choice.

---

## Minor

### m1. Garbled doc comment on `Parse::Full`
- **Location**: `syntax/src/worker.rs:41`
- **Guideline**: Readability.
- **Explanation**: The comment reads `/// The text was         && entry.parsed.and_then(|parsed| job.version.checked_sub(parsed)) == Some(job.edits.len() as u64) from scratch.` A code fragment from a botched edit was pasted into the sentence. The fragment also hints that a `checked_sub` version of line 223 existed.
- **Suggested improvement**: Restore it to "The text was parsed from scratch."

### m2. Unchecked subtraction can overflow
- **Location**: `syntax/src/worker.rs:223` (`job.version - parsed`)
- **Guideline**: Correctness (edge cases) and Error Handling.
- **Explanation**: The guard `entry.parsed == job.base` does not prove `job.version >= parsed`. The worker is a public API and `Job` has public fields. If `job.version < parsed`, this panics in debug builds, and in release it wraps and falls through to the wrong parse mode. The garbled comment in m1 shows `checked_sub` was the intended form.
- **Suggested improvement**: Use `job.version.checked_sub(parsed)` as in the comment.

### m3. A failure from a stale job can disable the new language
- **Location**: `syntax/src/document.rs:151-155` (`accept`), `syntax/src/worker.rs:58` (`Output`)
- **Guideline**: Correctness (race).
- **Explanation**: `update` calls `worker.forget` and `restart` when the language changes, but a job for the old language may still be in flight. `Output` carries no language or generation. If the old job finishes with `failure`, `accept` sets `self.failed = self.language`. `self.language` is now the new language, so the new language is never retried until `start_over`. A non-failing old output with a version in range is also accepted, so old-grammar spans show until the next answer.
- **Suggested improvement**: Put the `Language` (or a generation counter) in `Job` and `Output`, and ignore outputs that do not match `self.language`.

### m4. Stray `{ }` blocks and a duplicated doc paragraph in `DocumentSyntax`
- **Location**: `syntax/src/document.rs:141-143`, `174-178`, `199-201` (blocks around single assignments); `188-191` (`start_over` has two overlapping doc paragraphs)
- **Guideline**: Readability.
- **Explanation**: Bare blocks around one statement are leftovers from refactoring. The duplicated doc paragraph is confusing.
- **Suggested improvement**: Remove the blocks and merge the two doc paragraphs.

### m5. Worker failures are `String`s
- **Location**: `syntax/src/worker.rs:58` (`Output::failure: Option<String>`), `syntax/src/document.rs` (`error: Option<String>`, `take_error`)
- **Guideline**: Error Handling (generic String errors).
- **Explanation**: `SyntaxError` exists, but the worker flattens it with `to_string()` and also invents `"the highlighter vanished"` (`worker.rs:590`). Callers cannot match on the cause.
- **Suggested improvement**: Carry a `SyntaxError`, with a new variant for the vanished highlighter. If `Clone` or `PartialEq` is needed, wrap the sources in an `Arc` or keep a kind enum next to the message.

### m6. The worker thread has no panic recovery and is never joined
- **Location**: `syntax/src/worker.rs:95` (`thread::spawn`), lock comment at `:503`
- **Guideline**: Correctness and Error Handling.
- **Explanation**: `lock()` is poison-tolerant "so a panic in the worker thread must not take the app down". But if a tree-sitter call panics, the thread dies silently. Later jobs queue forever, `awaiting` stays true, and highlighting stops with no error shown. `Drop` also does not join the thread.
- **Suggested improvement**: Wrap `process` in `catch_unwind` and report it as a failure `Output`, or respawn the thread on submit. Join the handle in `Drop`.

### m7. Language detection is case-sensitive
- **Location**: `syntax/src/language.rs:69` (`from_path`)
- **Guideline**: Correctness (edge cases).
- **Explanation**: `Main.RS`, `README.MD` and `DATA.JSON` get no highlighting or comment toggling. The editor also targets Windows and the Windows file systems it supports are case-insensitive, where this is common.
- **Suggested improvement**: Compare `to_ascii_lowercase()` of the extension, or use `eq_ignore_ascii_case`.

### m8. Unguarded `u16` arithmetic in `tab_rects`
- **Location**: `ui/src/bar.rs:31`
- **Guideline**: Correctness (edge cases) and Error Handling.
- **Explanation**: `separator * (widths.len() as u16 - 1)` underflows for a bar with zero labels. `BarTabs` has public fields and no constructor, so it can be built empty. `widths.iter().sum::<u16>()` can also overflow, because each label width saturates at `u16::MAX`. The doc says "Empty for a placement that is not a bar with several views", but nothing enforces it.
- **Suggested improvement**: Return early when there are fewer than two labels. Use saturating or `u32` arithmetic for `total`.

### m9. `Option<Option<Color>>` as a return type
- **Location**: `ui/src/theme.rs:531` (`pane_background`), consumer `ui/src/backdrop.rs:25-34`
- **Guideline**: Types (named types over nested options).
- **Explanation**: `None`, `Some(None)` and `Some(Some(c))` are three states hidden in a doc comment, and `paint_pane` has to match on the nesting.
- **Suggested improvement**: Return `enum PaneBackground { Inherit, Transparent, Color(Color) }`.

### m10. Boolean parameters and fields where an enum reads better
- **Guideline**: Types and Functions (avoid boolean parameters).
- **Locations**:
  - `launch/src/build.rs:17` (`build_args(.., release: bool)`)
  - `launch/src/list.rs:62-64` (`release`, `from_file` fields)
  - `launch/src/file.rs` (`Patch.release`)
  - `ui/src/layout.rs:336`, `:816`, `:878` (`wide: bool` threaded through `size_for`, `place`, `place_children`)
  - `ui/src/pane_frame.rs:228` (`block(.., focused: bool)`)
  - `ui/src/hideable.rs:55` (`set_shown(bool)`)
  - `files/src/entries.rs:117` (`relocate(.., into_folders: bool)`)
  - `plugin-api/src/message.rs:518` (`ToPlugin::key(key, ctrl, alt, shift)`, three adjacent booleans)
  - `buffer/src/lib.rs:49-56` (three lock flags `read_only`, `locked`, `unlocked`)
- **Explanation**: Call sites like `relocate(a, b, c, false, f)` or `key("x", true, false, true)` are unreadable. The three buffer flags encode what is really a small state machine: Editable, LockedByFile, LockedByApp, UnlockedForRoot. Illegal combinations such as `locked && unlocked` are representable.
- **Suggested improvement**: Use small enums (`Profile::{Debug, Release}`, `Width::{Wide, Narrow}`, `Focus`, `Modifiers` via bitflags or a struct, `Lock`).

### m11. `collect` has seven parameters
- **Location**: `syntax/src/highlighter.rs:285`
- **Guideline**: Functions (avoid more than 6 parameters).
- **Explanation**: `query, styles, root, text, range, pattern_offset, found` is seven. `query` and `styles` always travel together, and so do `query` and `styles` in `InnerHighlights`.
- **Suggested improvement**: Bundle `(Query, Vec<Option<Style>>)` as a `StyledQuery` struct with a `collect` method, which removes two parameters.

### m12. `removal::TrashFn` and `move_to_trash` use `String` errors
- **Location**: `files/src/removal.rs:7`, `:9`
- **Guideline**: Error Handling (generic String errors).
- **Explanation**: `Result<(), String>` loses the source error, while `remove_permanently` in the same file returns `io::Result`.
- **Suggested improvement**: Define `TrashError(#[from] trash::Error)` (thiserror), and make `TrashFn` return it.

### m13. App-specific variants in the files crate's `EntryError`
- **Location**: `files/src/entries.rs:20-26`
- **Guideline**: API Design (poor encapsulation, leaking a higher layer).
- **Explanation**: `NotAPlace` ("use 42 or 42:7"), `BadThemeName`, `NotAFile` and `NotAFolder` are never produced by this crate. They are built in `src/app/open.rs`, `config_files.rs` and `popups/new_entry.rs`. A leaf crate's error type has become the application's grab bag.
- **Suggested improvement**: Move those variants to an app-level error, and keep `EntryError` to what `files::entries` can produce.

### m14. Test-only accessor in the public API of `Secret`
- **Location**: `files/src/rootsave.rs:59` (`pub fn raw_for_test`)
- **Guideline**: API Design (public implementation details).
- **Explanation**: It exposes the secret bytes of a type whose purpose is to contain them. It is called from `src/tests/root_save_tests.rs:223`.
- **Suggested improvement**: Gate it with `#[cfg(any(test, feature = "test-support"))]`, as `clipboard` does for `Memory`.

### m15. `Secret` reallocation leaves unwiped copies
- **Location**: `files/src/rootsave.rs:18-30` (`push`)
- **Guideline**: Correctness against the module's own stated goal ("wiped from memory at once").
- **Explanation**: The `Vec<u8>` starts empty and grows through `extend_from_slice`. Each reallocation frees the old buffer without wiping it, so earlier password bytes stay in freed heap memory. Only the final buffer is zeroed on drop.
- **Suggested improvement**: Allocate a fixed capacity up front with `Vec::with_capacity(…)` and refuse growth beyond it, or wipe the old buffer before growing.

### m16. `files` crate has no tests of its own
- **Location**: `files/` (no `tests` module in any file); tests live in the root package: `src/tests/{paths,watcher,root_save,create,rename,move,delete}_tests.rs`
- **Guideline**: Testing (tests next to the source), AGENTS.md ("unit tests live in sibling tests.rs").
- **Explanation**: `cargo test -p trcode-files` runs nothing. `root_save_tests` builds an `App` to test a `sudo` script and a `Secret` type, so the crate cannot be verified without the editor.
- **Suggested improvement**: Move the entry, path, removal, watcher and `RootWriter` tests into `crates/files/src/tests/`, and keep only the `App` integration in the root.

### m17. Byte-shift logic and `Change` duplicated across crates
- **Location**: `jumplist/src/lib.rs:26` and `:166`; `breakpoints/src/lib.rs:36` and `:426`; `buffer/src/edit.rs:54` (`EditInfo::remap_byte`)
- **Guideline**: Maintainability (duplication).
- **Explanation**: The same three-branch shift function is written three times, and two identical `Change` structs are declared. A fix in one (for example the insertion-at-start semantics) must be repeated by hand. `breakpoints` already diverges (`shift_mark` special-cases a line break typed at a line start).
- **Suggested improvement**: Put one `Change` and `shifted` in a low-level crate (`trcode-ids` or `trcode-buffer`), and have the other crates use it.

### m18. `JumpList::reopened` takes `&PathBuf`
- **Location**: `jumplist/src/lib.rs:146`
- **Guideline**: Ownership and API Design (take `&Path`). Clippy `ptr_arg`.
- **Suggested improvement**: `path: &Path`, comparing with `place.path.as_deref() == Some(path)`.

### m19. Undo and redo are not atomic on error
- **Location**: `buffer/src/history.rs:127` (`undo`), `:140` (`redo`)
- **Guideline**: Correctness (error paths) and Error Handling.
- **Explanation**: `Buffer::apply` documents "on error the buffer is unchanged", and `Transaction` rolls back. But `undo` pops the record first and then calls `revert(...)?`. If it fails halfway, the record is lost and the buffer is partly reverted. `redo` has the same problem with `self.apply(edit)?`. This needs an invariant violation to happen, but the result would be silent text and history divergence.
- **Suggested improvement**: Push the record back on error, or treat a failure as an invariant violation with a `debug_assert!`, as `Transaction::drop` does.

### m20. Contradictory docs on `unlock_for_root`
- **Location**: `buffer/src/lib.rs:113-123`
- **Guideline**: Readability.
- **Explanation**: The doc ends mid-thought: "A buffer the app locked stays locked. Whether the text can be edited now." `sync_read_only` and `unlock_for_root` also mutate state and return a status, which mixes command and query.
- **Suggested improvement**: Fix the sentence. Optionally return `Result<(), Locked>` or split the query out.

### m21. `RunList::build` is long and nested, with a repeated struct literal
- **Location**: `launch/src/list.rs:187-244` (58 lines, about four levels of nesting); literals at `:127-139`, `:141-151`, `:227-236`
- **Guideline**: Functions (one responsibility, small, early returns), Readability.
- **Explanation**: Override, command and missing handling are inlined in one `match` with `continue`s. The same eight-field `RunConfig` literal appears three times.
- **Suggested improvement**: Extract `apply_override` and `add_command` helpers, and add a `RunConfig::new(id, name, what, from_file)`.

### m22. `parse_all` takes an iterator of `&String`
- **Location**: `plugin-api/src/run.rs:121`
- **Guideline**: API Design (non-idiomatic signature).
- **Explanation**: `impl IntoIterator<Item = &'a String>` forces callers holding `&[&str]` or `Vec<&str>` to allocate or map.
- **Suggested improvement**: `impl IntoIterator<Item = S>` with `S: AsRef<str>`.

### m23. `Manifest` is stringly typed although typed parsers exist
- **Location**: `plugin-api/src/manifest.rs:55` (`abi: String`), `:63` (`capabilities: Vec<String>`), `:58` (`requires`)
- **Guideline**: Types (strong types over primitives).
- **Explanation**: The same crate provides `Abi: FromStr` and the `Capability` enum, but `Manifest` carries unparsed strings and leaves validation to the caller. That is deliberate ("judging it is the editor's"), but it means the typed model is never the thing the editor holds. `Manifest` is also `#[non_exhaustive]` and has no constructor, although `lib.rs:10` says every struct can be built with its `new`.
- **Suggested improvement**: Deserialize into `Abi` (serde `try_from` or `FromStr`) and `Vec<Capability>` (keeping `Unknown`). Add `Manifest::new`, or correct the doc.

### m24. `layout.rs` and `theme.rs` violate the file layout, and the layout error message contradicts the check
- **Guideline**: Style (file layout: globals, then `pub`, then private), Correctness (message).
- **Locations**:
  - `ui/src/layout.rs:469`, `:490-491`, `:522`: `RIGHT_BAR_MIN_WIDTH`, `LEAST_BAR`, `LEAST_REST` and `DEFAULT_LAYOUT_RON` are in the middle of the file, after about 450 lines of types.
  - `ui/src/theme.rs:156-410`: roughly 250 lines of private spec, palette and merge types sit between the public `Rgb`/`BackgroundPane`/`ThemeError` and `impl Theme` at `:412`.
  - `ui/src/layout.rs:50` says only Explorer, Terminal and Plugin views can be bar views. But `validate` (`:741-751`) rejects only Editor, StatusBar and MenuBar, so Errors and Debug are accepted.
- **Explanation**: Constants should open the file, and the public `Theme` surface should come before the TOML plumbing. The message tells users something the code does not enforce.
- **Suggested improvement**: Move consts to the top and private spec types after the public API (or into `theme/spec.rs`). Fix the message to match the validation.

### m25. Other file-layout drift (public items interleaved with private helpers)
- **Guideline**: Style (file layout).
- **Locations**:
  - `buffer/src/lib.rs`: private `ascii_line_len` (`:189`) and `apply` (`:233`) sit between public methods, and `line_content` (`:360`) is public after private helpers.
  - `breakpoints/src/lib.rs:49-77`: private `Verification`, `Mark` and `File` sit between public types.
  - `files/src/entries.rs:72`: private `inside` between public `create` and `rename`.
  - `files/src/rootsave.rs:180`, `:211`: a second `impl RootWriter` and an `impl Secret` for one private method, instead of merging them.
  - `plugin-api/src/message.rs:485` and `:561`: `impl WireView` is split in two with other types between.
- **Suggested improvement**: Consolidate impl blocks and move private helpers to the bottom.

### m26. Test depends on the repository layout and on `cargo`
- **Location**: `launch/src/tests/target.rs:201-208` (`this_repository_has_the_editor_among_its_targets`)
- **Guideline**: Testing (not brittle or coupled to implementation details).
- **Explanation**: It runs a real `cargo metadata` against `CARGO_MANIFEST_DIR/../..` and asserts a package named `terminal_code`. Renaming the package or running tests without cargo on `PATH` breaks it, and the check says nothing about `launch` behavior. It only reads the repo, so there is no repo-writing problem. The other fs tests use `tempfile` correctly.
- **Suggested improvement**: Build the fixture in a `tempdir` (as the neighbouring tests do), or move this check to the root package as an integration test.

---

## Suggestions

### s1. `Highlighter` compiles the highlight query once per document
- **Location**: `syntax/src/worker.rs:201` (`Highlighter::new` per key); `syntax/src/highlighter.rs:72`
- **Guideline**: Performance (only with evidence) and Ownership.
- **Explanation**: `kept` is keyed by document, so N open Rust files compile the Rust query N times (and Markdown compiles two). Compiling queries is not cheap.
- **Suggested improvement**: Measure first. If it matters, cache `Arc<Query>` and styles per `Language` in the worker. Per-document state would then be just the `Parser` and `Tree`.

### s2. The worker picks the next job from a `HashMap`
- **Location**: `syntax/src/worker.rs:176` (`jobs.keys().next()`)
- **Explanation**: Iteration order is arbitrary, so with several documents re-submitting, one can wait longer than needed. A document being typed in can be starved by background documents.
- **Suggested improvement**: Keep a FIFO of keys (`VecDeque`) alongside the map.

### s3. `Buffer::disk_change` reads and hashes the whole file on every check
- **Location**: `buffer/src/file.rs:122-143`
- **Explanation**: If this is polled (the app has a file watcher), large files cost a full read each time. A modification-time and length check could short-circuit the hash. Measure before changing.

### s4. `&dyn Fn` where generics fit the guidelines
- **Location**: `files/src/entries.rs:102`, `:122`, `:151` (`same_entry: &dyn Fn(&Path, &Path) -> bool`); `ui/src/layout.rs:187-188` (`Scene` holds two `&dyn Fn`)
- **Guideline**: Types (generics / `impl Trait` over `dyn`).
- **Suggested improvement**: `impl Fn` parameters for entries. `Scene` could be a small trait implemented by the app, which also removes `|_| true` and `|_| 0` closure literals at the call sites.

### s5. Public constructors can panic in `Default`
- **Location**: `ui/src/layout.rs:688` (`LayoutTree::default`), `ui/src/theme.rs:478` (`built_in`), `:599-605` (`Theme::default`)
- **Explanation**: They panic on a malformed embedded file, which is an acceptable invariant but should be covered by a test that runs in CI. The message at `theme.rs:602` has a typo ("tyring").
- **Suggested improvement**: Fix the typo. Ensure the tests parse every embedded layout and theme.

### s6. `PaneFrame::border_style_slot` allocates on each draw
- **Location**: `ui/src/pane_frame.rs:266-273`
- **Explanation**: Two `String`s are built per pane per frame (`format!` plus `to_string`) only to look up a slot. The result could be a `Cow<str>`, or the focused slot name could be precomputed in `PaneFrame` at resolve time. Not a problem at current sizes, so measure first.

### s7. `Capability::parse` and `name` duplicate the serde names
- **Location**: `plugin-api/src/message.rs:612-681`
- **Guideline**: Standard APIs (`FromStr`/`AsRef<str>`, do not invent custom APIs).
- **Explanation**: The wire names live in both `#[serde(rename)]` and `name()`. A new capability must be added in three places (enum, `ALL`, `name`).
- **Suggested improvement**: Implement `FromStr` and `Display`, and add a test that checks `name()` against the serde output for every entry in `ALL`.

### s8. `MessageError::Invalid(String)`
- **Location**: `plugin-api/src/message.rs:14`
- **Explanation**: Flattening `serde_json::Error` to a `String` keeps `Clone`/`Eq` but drops the source and line or column. Consider a kind enum or storing `(line, column, message)`.

### s9. wasm `handle` ignores its pointer argument
- **Location**: `plugin-api/src/guest.rs:65` (`deliver(_ptr, len)`)
- **Explanation**: The exported `handle(ptr, len)` takes the message from the thread-local inbox and ignores `ptr`. That works only if the host always writes to the last `alloc`ed buffer. The contract is not documented, and `len.min(inbox.len())` silently truncates a mismatched length.
- **Suggested improvement**: Document the protocol on `alloc`/`handle`, or validate `ptr == inbox.as_ptr()`.

### s10. Tests: `unwrap()` density and file naming
- **Guideline**: Testing (avoid `unwrap()` where an assertion is feasible; tests in `{name}_tests.rs`).
- **Explanation**: Many unwraps are justified (tempdir, fixture setup). But `plugin-api/src/tests/message_tests.rs` (72), `ui/src/tests/theme_tests.rs` (56), `ui/src/tests/layout_tests.rs` (41) and `buffer/src/tests/file_tests.rs` (75) unwrap the result under test (`to_bytes().unwrap()`, `from_toml(..).unwrap()`). A failure there reports a panic location, not an assertion. Test files are also named inconsistently: `jumplist/src/tests.rs`, `breakpoints/src/tests.rs`, `launch/src/tests/{build,file,list,target}.rs` instead of the `{name}_tests.rs` convention.
- **Suggested improvement**: Use `-> Result<(), _>` tests with `?`, or `assert_eq!(.., Ok(..))`, for the value under test. Rename test files to the convention.

---

No repo-writing filesystem tests were found in the scoped crates. They use `tempfile`. No `unsafe` without a SAFETY comment was found: `files/src/rootsave.rs:79` and `plugin-api/src/guest.rs:90` both have one. No unjustified `unwrap`/`expect`/`panic!` in non-test code beyond the embedded-default panics in s5.

---

<!-- Part C -->
# Review C: `src/app/` (non-test code, about 18k lines)

Reviewed against `docs/Guideline PROMPT REVIEW.md` and `docs/Guideline PROMPT CODE.md`.

## Overall assessment

The module is in good shape on the points the guidelines care most about. In non-test code under `src/app/` there is no `unwrap()`, `expect()`, `panic!()`, `todo!()` or `unsafe` at all. Early returns and `let ... else` are used consistently. Async work (LSP, git, plugins, debugger) is modelled with explicit state machines (`Stage`, `Waiting`, `Kind`) and stale-request timeouts. Doc comments explain intent well. I found no memory-safety problem and no data-loss bug.

The real weaknesses are structural:

1. A few very large functions (`handle_mouse` at 287 lines, `debug_answers` at 129, `handle_key` at 123, `format_progress` at 84).
2. Copy-paste duplication across the LSP feature modules and the three debugger tree views.
3. Stringly-typed control flow (the `"stopped"` sentinel, `Result<_, String>`, `"id:place"` action payloads).
4. Boolean and positional-tuple parameters.
5. File layout that often breaks "globals, then `pub`, then private".

There is one concrete bug: `plugin_keys.rs` writes a key into the user's TOML without escaping, which can corrupt `keymap.toml`.

Both known clippy warnings are included below as findings 7 and 8.

Severity counts: Critical 0, Major 6, Minor 30, Suggestion 8.

---

## Major

### 1. Key written to `keymap.toml` without escaping can corrupt the user's keymap
- **Severity:** Major
- **Location:** `src/app/plugin_keys.rs:521-523` (with `src/keymap/key.rs:114`)
- **Guideline:** Correctness: invalid assumptions, edge cases.
- **Explanation:** `write_open_key` builds `keys = "{chord}"` and `open_plugin = "{id}"` with `format!` and no TOML escaping. `KeyChord`'s `Display` prints `KeyCode::Char(c)` raw, except for space and `+`. A chord such as `alt+\` or `alt+"` therefore writes `keys = "alt+\"` or `keys = "alt+""`, which is invalid TOML. `check_key` accepts any char with ctrl, alt or super held and checks only whether the chord is already bound. After the append, `reload()` re-reads the file and falls back to the built-in keymap, so the user's custom bindings stop applying until they fix the file by hand. A plugin folder name containing `"` or `\` has the same problem through `id`.
- **Suggested improvement:** Build the entry with the `toml` crate (`toml::Value::String` or `toml_edit`). Or escape `\` and `"` in both values. Add a test that binds `alt+\`.

### 2. `handle_mouse` is a 287-line function with repeated boilerplate
- **Severity:** Major
- **Location:** `src/app/mouse.rs:93-379`
- **Guideline:** Style: function size, nested control flow; Functions: one responsibility.
- **Explanation:** One function holds about 25 ordered `if handle_x(event) { self.needs_redraw = true; return; }` stages, then the whole editor-pane match. The condition `matches!(popup, Popup::None | Popup::PluginView(_))` is copied three times (lines 138-143, 203-208, 214-219). Order is semantically significant, so it is hard to change safely. `let sideways = extend;` (line 310) is a pointless alias.
- **Suggested improvement:** Split into stages that return a small enum (`Handled` or `NotHandled`). Run them from a table or `.find_map` chain, and set `needs_redraw` in one place. Extract `popup_allows_plugin_input()`. Move the editor-pane `match event.kind` into its own method.

### 3. `debug_answers` is a 129-line, near-identical match arm per request kind
- **Severity:** Major
- **Location:** `src/app/debug.rs:448-576`
- **Guideline:** Style: function size; Readability: unnecessary complexity.
- **Explanation:** Each of 14 `Kind` variants repeats: take the reply, `try_take`, put it back and rebuild the variant when still pending, otherwise handle it and `continue`. The `Variables` arm rebuilds all four fields only to move them back. The `Continue` and `Step` arms are identical, and the `Launch` and `ConfigurationDone` arms are almost identical. Adding a request kind means copying another 8-12 lines, and forgetting the "put it back" half drops a reply silently.
- **Suggested improvement:** Give `Kind` a `poll(&mut self) -> Option<Answer>` (or split reply from metadata), where `Answer` is an enum of `Result`s. Then `debug_answers` becomes `retain`-style: poll each waiting item, and dispatch finished ones with one `match`.

### 4. The "ask for children and map `Variable` to `Incoming`" logic exists three times
- **Severity:** Major
- **Location:** `src/app/debug.rs:820-863` (`debug_ask_variables`, `debug_variables`); `src/app/debug/evaluate.rs:252-308` (`debug_ask_hover_variables`, `debug_hover_variables`); `src/app/debug/watch.rs:713-760` (`debug_ask_watch_variables`, `debug_watch_variables`)
- **Guideline:** Maintainability (Major: hurts maintainability); Standard APIs (prefer `From`).
- **Explanation:** The same 15-line `VariablesArguments::page/of` and `Kind::Variables { .. }` push is copied per `Target`. The `Incoming { name, value, type_name, reference, total: indexed.or(named) }` mapping is pasted verbatim at `debug.rs:848-858` and `evaluate.rs:287-297`, with a variant in `watch.rs`. A protocol change (for example how `total` is chosen) must be made in three places. `Target` is already in `Kind::Variables`, so the dispatch point exists.
- **Suggested improvement:** Add `impl From<Variable> for Incoming` (the `evaluate::incoming` helper already does this for `EvaluateResponse`). Add one `debug_ask_children(active, read, target)` and one `children_answered(target, ...)` that picks the right tree. The three copies reduce to a `match target` on which tree to touch.

### 5. A `"stopped"` string sentinel decides control flow; install results are `Result<(), String>`
- **Severity:** Major
- **Location:** `src/app/plugin_list.rs:673-676` (creation) and `748` (`Err(reason) if reason == "stopped"`); the `String` results flow through `JobEvent::{Index, SourceIndex, Readme, Done}`.
- **Guideline:** Error Handling: generic `String` errors; Types: strong types over primitives.
- **Explanation:** `InstallError::Cancelled` is converted to the text `"stopped"` on the worker thread, and the UI thread compares against that text to tell "cancelled" from "failed". Rewording the message silently turns a cancel into an error popup. The typed `InstallError` already exists, but is flattened to `String` before crossing the channel, losing structure.
- **Suggested improvement:** Make `JobEvent::Done.result` a `Result<(), InstallOutcome>` (or send `InstallError` itself if it is `Send`). Have an `enum { Cancelled, Failed(String) }` do the matching. The same applies to the other `JobEvent` payloads.

### 6. `format_progress` hides a state machine behind `mem::replace`
- **Severity:** Major
- **Location:** `src/app/formatting.rs:338-421`
- **Guideline:** Readability: nested control flow; Style: function size; Ownership.
- **Explanation:** `item.state` is overwritten with `State::Waiting` up front, then conditionally restored in 3 of the paths. In the `Waiting` arm an `if running {..} else if !starting {..} else if elapsed {..} else { return None }` chain interleaves early returns with fall-through code (lines 347-365), which is easy to misread, and the real work (`lsp_sync`, request) only follows when the first branch did not return. The pattern risks losing the pending request if a path is added that forgets the restore.
- **Suggested improvement:** Split into `progress_waiting(&mut Pending) -> Option<Done>` and `progress_asked(...)`. Match on `&mut item.state` without `mem::replace`. Extract a `server_ready(&key) -> Result<(), &'static str>` (also usable by finding 11).

---

## Minor

### 7. Clippy `unit_arg`: `Some(scroll.lines(-1))` wraps a unit value
- **Severity:** Minor
- **Location:** `src/app/hover.rs:512-515`
- **Guideline:** Readability; Types.
- **Explanation:** `scroll.lines()` and `pages()` return `()`, so `scrolled: Option<()>` is used only as a "key was handled" flag. This is the known clippy warning.
- **Suggested improvement:** Compute `let step = match key.code { Up => Some(...), ... }` (only the action), then call once, or `match` with a `return true` after each call. For example `KeyCode::Up => scroll.lines(-1), ... _ => break`, or `let handled = matches!(key.code, Up | Down | PageUp | PageDown)`.

### 8. Clippy `collapsible_match`: `if` inside a match arm
- **Severity:** Minor
- **Location:** `src/app/plugins.rs:581-586`
- **Guideline:** Readability: nested control flow.
- **Explanation:** `WireMenuItem::Separator => { if !matches!(...) { push } }` can be a guard. This is the known warning.
- **Suggested improvement:** `WireMenuItem::Separator if !matches!(entries.last(), None | Some(Entry::Separator)) => entries.push(Entry::Separator),` followed by `_ => {}`.

### 9. `Result<_, String>` in several internal APIs
- **Severity:** Minor
- **Location:** `src/app/run.rs:602` (`spawn_program`); `src/app/debug.rs:337` (`start_debuggee_in_tab`); `src/app/formatting.rs:463` (`apply_format`); `src/app/plugin_keys.rs:483` (`check_key`); `src/app/jumps.rs:162` (`go_to_place_of`); `BuildOutcome::NotStarted(String)` (`run.rs:52`)
- **Guideline:** Error Handling: generic `String` errors; suggest concrete error types.
- **Explanation:** All of these just forward a message for display, but the callers lose the ability to match on cause. `apply_format` even produces `"the file has no name"` twice (lines 472-473). The module already has a good typed example (`EditError`).
- **Suggested improvement:** Small `thiserror` enums (`SpawnError`, `FormatError`, `KeyRefusal`, `JumpError`) whose `Display` is the current text.

### 10. The "server for this document that offers X" lookup is copied five times, plus pass-through wrappers
- **Severity:** Minor
- **Location:** `src/app/completion.rs:413-425`; `src/app/definition.rs:306-311`; `src/app/rename.rs:137-142`; `src/app/hover.rs:534-539`; `src/app/code_actions.rs:174-179` (`formatting.rs:271` is a sixth variant). Wrappers: `hover.rs:542`, `definition.rs:314`, `completion.rs:428` and `444`.
- **Guideline:** Maintainability; Readability: unnecessary complexity.
- **Explanation:** Each body is `targets.get(&doc)? -> target.as_ref()? -> manager.as_ref()?.offers_x(&key) -> clone key and uri`. The `request_*` helpers only forward to `manager.request_*`.
- **Suggested improvement:** One `fn target_offering(&self, doc, impl Fn(&Manager, &ServerKey) -> bool) -> Option<(ServerKey, String)>` in `lsp.rs`. Call `self.lsp.manager.as_ref()?.request_x(..)` directly, or drop the wrappers.

### 11. Duplicated predicates in `formatting.rs`
- **Severity:** Minor
- **Location:** `src/app/formatting.rs:50` and `109` (identical bodies); preflight at `150-168`, `182-193`, `225-237`
- **Guideline:** Maintainability; API Design.
- **Explanation:** `formatting_pending` and `formatting_busy` have identical bodies and both are called. Three entry points each repeat "running and does not offer X, or neither running nor starting, then tell the user".
- **Suggested improvement:** Keep one of the two predicates. Extract `fn check_server(&self, key, feature: Feature) -> Result<(), &'static str>` and reuse it in `format_progress` as well.

### 12. `Waiting { config: Option<ConfigId>, step }` allows invalid combinations
- **Severity:** Minor
- **Location:** `src/app/run.rs:57-63`, `157-162`
- **Guideline:** Types: strong types, make invalid states unrepresentable.
- **Explanation:** `Step::List` always has `config: None`, and `Asking`, `Trusting` and `Formatting` always have `Some`. The `(Step::Formatting, None) => false` arm (line 161) is unreachable but would leave the run stuck forever if it ever happened. `step_after_saves` documents "returns whether the next step can be taken" but always returns `false`.
- **Suggested improvement:** `enum Step { List, Asking(ConfigId), Trusting(ConfigId), Formatting(ConfigId) }`. Make `step_after_saves` return `()`.

### 13. `run_build` maps a `try_wait` error to "Cancelled" and may hang
- **Severity:** Minor
- **Location:** `src/app/run.rs:757-761`
- **Guideline:** Error Handling: hidden errors; Correctness.
- **Explanation:** `Err(_) => break None` hides the I/O error and reports `BuildOutcome::Cancelled` (which `run_built` silently ignores). The child is neither killed nor reaped on that path. The reader threads are then joined (lines 763-766) and block until the pipes close, so the build thread can hang while cargo keeps running.
- **Suggested improvement:** On `Err`, kill and wait the child as in the cancel path, and return `BuildOutcome::NotStarted(error.to_string())` (or a new `Failed` variant carrying the error).

### 14. `run_built` threads an unnamed `(String, u32)` tuple through a 55-line match
- **Severity:** Minor
- **Location:** `src/app/run.rs:555-590`
- **Guideline:** Readability; Types: named structs.
- **Explanation:** `said.0` and `said.1` (message and exit code) are non-obvious, and the `Built` arm mixes an early `return` with a value-producing arm. `run_built` also does `builds.get()` and then `builds.remove()` for the same key (lines 540-546).
- **Suggested improvement:** Return `Option<(Message, ExitCode)>` from a helper `fn failure_text(outcome) -> (String, u32)`. Use one `remove` plus a generation check, re-inserting if stale.

### 15. Boolean parameters where an enum would read better
- **Severity:** Minor
- **Location:** `plugin_list.rs:643` and `766` (`update: bool`); `debug.rs:754` (`keyboard_to_editor`); `terminal_pane.rs:330` (`say`); `popups/find.rs:113` (`forward`); `diagnostics.rs:69` (`next`); `completion.rs:320` (`next`); `hover.rs:470` (`down`); `git.rs:42` (`now`); `mouse.rs:16` (`change: Option<bool>`); `keys.rs:333-336, 357` (`find_step(true)`, `step_diagnostic(false)`, `copy_terminal_selection(true)`); `ToPlugin::key(.., ctrl, alt, shift)` in `plugins.rs:650-653`
- **Guideline:** Functions: avoid boolean parameters; Types: enums over booleans.
- **Explanation:** Call sites like `self.find_step(true)` and `self.start_install(entry, false)` are unreadable without opening the callee. `Option<bool>` for "a change the terminal has not been told about" is three-state by accident.
- **Suggested improvement:** `Direction::{Next, Previous}`, `Install::{Fresh, Update}`, `Verbosity::{Quiet, Say}`, `MouseChange::{On, Off}`. The same applies to the `isize` step parameters (`step_terminal_tab`, `cycle_bar_view`, `step_search_result`, `resize_side_pane(5)`), where only the sign or a magic number matters.

### 16. Positional tuples with swapped component order
- **Severity:** Minor
- **Location:** `src/app/terminal_pane.rs:296-311` and `350`; `src/app/state.rs` (`plugin_areas: HashMap<_, (Rect, Rect)>`); `src/app/plugins.rs:35` (`sized: (u16, u16)`); `FIRST_SIZE`, `terminal_dimensions() -> (u16, u16)`
- **Guideline:** Types: named structs, strong types.
- **Explanation:** `screen_cell` returns `(column, row)`, but `begin_selection` and `select_word` take `(row, column)`. `begin_terminal_selection` swaps by hand (`let (row, column) = (cell.1, cell.0)`). A swapped pair compiles and misplaces selections. `(Rect, Rect)` does not say which is the body.
- **Suggested improvement:** Use `ratatui::layout::{Position, Size}`, or a small `Cell { row, column }` type. Use `Size` for `sized` and the terminal dimensions.

### 17. The same wheel-step constant is defined four times with different types
- **Severity:** Minor
- **Location:** `src/app/hover.rs:529` (`WHEEL_LINES: isize = 3`, declared mid-file); `mouse.rs:62` (`i16`); `terminal_pane.rs:20` (`isize`); `debug_hover.rs:361` (`WHEEL_ROWS: isize`)
- **Guideline:** Style: globals first; Maintainability.
- **Explanation:** One user-facing setting (lines per notch) is spread over four constants, so changing one leaves the others behind. The `hover.rs` one sits after `impl` blocks, against the globals-first rule.
- **Suggested improvement:** One shared `pub(super) const WHEEL_LINES` in `mouse.rs`, with the type conversion at the use site.

### 18. File layout deviates from "globals, then `pub`, then private"
- **Severity:** Minor
- **Location:** `src/app/mod.rs:26-85` (a `mod` list with `#[cfg(test)]` shims and a `pub(crate) use` stuck in the middle of the module list, then `use` after it); `mouse.rs:62` (const), `382-383` (`#[cfg(test)] mod tests;` mid-file); `debug.rs:66-67` (`pub(super) mod evaluate; mod watch;` after the first `impl`); `hover.rs:529`; `formatting.rs` (private `Pending`, `Then`, `State`, `Done` types between two `impl App` blocks); `rename/apply.rs:132-165` (a `pub fn` between private structs, `Plan` impl split in two); `plugin_list.rs` (private fns between `pub(super)` items); `popups/mod.rs`
- **Guideline:** Style: file layout.
- **Explanation:** Several files interleave constants, `mod` declarations, public and private items, and several have multiple `impl App` blocks. This makes the public surface hard to find.
- **Suggested improvement:** Put `mod` and `use` first, then constants, then pub(`super`/`crate`) types and impls, then private helpers, with `#[cfg(test)] mod tests;` last.

### 19. Word-boundary logic is re-implemented in five places
- **Severity:** Minor
- **Location:** `completion.rs:397` and `hover.rs:441` (identical `is_word_char`); `definition.rs:295` (`word_at`, inline copy of the predicate); `rename.rs:401` (`word_range`); `completion.rs:402` (`word_start`); `hover.rs:447` (`word_around`). The "on the word or just before it" idiom is duplicated in `definition.rs:68-72` and `hover.rs:402-409`.
- **Guideline:** Maintainability; Readability.
- **Explanation:** Changing what counts as a word (for example `-` in CSS) needs five edits, and the copies already differ in bounds handling.
- **Suggested improvement:** A tiny `app/words.rs` (or one pub(super) set in `definition.rs`) with `is_word_char`, `word_around`, and `word_near(text, byte)`.

### 20. Plugin pane hit-testing and git-write dispatch contain duplicated or stringly code
- **Severity:** Minor
- **Location:** `src/app/plugins.rs:462-488` and `538-552` (the same find-leaf, `rows_scroll`, clicked-row computation); `plugins.rs:45-50, 202-207` (`RemoteJob.what: &'static str` plus `op` matched twice)
- **Guideline:** Maintainability; Types.
- **Explanation:** `click_message` duplicates `rows_row_at`. `plugin_git_write` matches `op` once to get a `"fetch"` or `"pull"` string and again inside the thread. Separately, `Cancel` is refused with `NoRepository` when there is no root, although it needs none.
- **Suggested improvement:** Make `click_message` call `rows_row_at` for the `Rows` arm. Give `WriteOp` a `remote_name() -> Option<&'static str>` (or an enum `Remote`). Check `Cancel` first.

### 21. Key and mouse handling for list popups is copy-pasted
- **Severity:** Minor
- **Location:** `src/app/code_actions.rs:316-370` and `definition.rs:185-238` (and `run_picker.rs`, which looks the same)
- **Guideline:** Maintainability.
- **Explanation:** `with_action_list` and `with_definition_list` are the same function. `handle_actions_mouse` and `handle_definitions_mouse` are the same function over different popup types, with the same Previous, Next, PageUp, PageDown and Cancel mapping.
- **Suggested improvement:** A trait or generic helper over a shared "selectable list" interface (`step`, `page`, `select_at`, `contains`) so one handler serves both.

### 22. Misplaced and garbled doc comments
- **Severity:** Minor
- **Location:** `src/app/git.rs:111-116` (the "Tells of a problem that is not final…" doc sits on `git_generation`, while `git_problem` at 130 has none); `src/app/hover.rs:363-366` (two sentences both start with "`f6`:", merging two drafts)
- **Guideline:** Readability.
- **Explanation:** The first one documents the wrong function and can mislead a reader. The second reads as contradictory.
- **Suggested improvement:** Move and merge the comments.

### 23. Update-available rule and plugin record lookup are duplicated
- **Severity:** Minor
- **Location:** `src/app/plugin_list.rs:57-63` vs `450-455` (`next_for` re-implements `update_for`'s four filters); `src/app/plugin_manager.rs:400-402` (`saved_plugin_record` is a pure alias of `saved_plugin`)
- **Guideline:** Maintainability; API Design: do not invent redundant APIs.
- **Explanation:** If `update_for` changes (for example a new filter), `next_for` would offer a different update than `row_of_installed` shows.
- **Suggested improvement:** `let update = entry.and_then(|e| update_for(installed, Some(e))).is_some()`. Delete `saved_plugin_record` and call `saved_plugin`.

### 24. Test-only behaviour inside production code, and string-coupled status
- **Severity:** Minor
- **Location:** `src/app/plugin_list.rs:39` (`cfg!(test)` in `Source::default`); `plugin_list.rs:143` with `693` (`note.starts_with("offline")` decides `warn`; the text is written at 693)
- **Guideline:** API Design; Types.
- **Explanation:** `cfg!(test)` makes the production `Default` differ from the tested one. The warn level depends on the wording of a message produced elsewhere.
- **Suggested improvement:** Default is `Some(URL)`. Tests build `Source` through the existing `#[cfg(test)] Source::local` (or `Store::with_source`). Store `note` as `enum Note { Asking, Offline(String) }`.

### 25. Other oversized functions
- **Severity:** Minor
- **Location:** `keys.rs:149-271` (`handle_key`, 123 lines of ordered special cases) and `319-469` (`dispatch`, 151-line match with magic numbers `resize_side_pane(5)` and `resize_focused_pane(50)`); `plugin_list.rs:890` (`describe_plugin`, 94 lines), `683` (`plugin_job_event`, 80), `472` (`handle_plugin_list_key`, 72); `mod.rs:195` (`run`, 91, six hand-written deadline arms), `484` (`handle_event`, 76); `hover.rs:81` (`hover_pointer_moved`, 64); `terminal_keys.rs:31`; `panels.rs:188` (`focus_toward`, 60); `dividers.rs:114`
- **Guideline:** Style: function size.
- **Explanation:** These are long but flat. `handle_key` is the worst, as a 12-stage precedence chain whose order is only documented in comments.
- **Suggested improvement:** `describe_plugin` becomes `head_lines()` plus `body()`. `handle_key` becomes a list of pre-handlers. Name the resize steps as constants. `run` gets a `next_deadline()` returning the earliest `Option<Instant>` instead of six `sleep_until` arms.

### 26. Debugger errors are dropped without a message
- **Severity:** Minor
- **Location:** `src/app/debug.rs:652-654` (`debug_threads`: `result.ok()`), `803-805` (`debug_scopes`: `let Ok(..) else return`), `835` and `861` (`Err(_)` in the variables paths), `328` (`let _ = respond_run_in_terminal`)
- **Guideline:** Error Handling: hidden errors.
- **Explanation:** If `threads` or `scopes` fails after a stop, the pane shows "Stopped" with an empty stack and no explanation. Variable failures show only a generic marker.
- **Suggested improvement:** `notify_error(format!("could not read the threads: {error}"))` like `debug_stack` already does. Keep the `respond_run_in_terminal` result and end the session on error.

### 27. Small defects in `code_actions.rs`
- **Severity:** Minor
- **Location:** `src/app/code_actions.rs:488-500` and `533-535`
- **Guideline:** Correctness: edge cases; Readability.
- **Explanation:** When the first `open_path` fails, `names` is empty but the code still notifies `"opened  for this action"` (two spaces) before checking `names.len() < closed.len()`. `apply_action` returns `Ok(String::new())` when there is no edit; `perform` only calls it with an edit, so the branch is dead and would notify an empty message. `after_second_answer` clones a `CodeAction` just to move `actions` afterwards.
- **Suggested improvement:** Notify only when `!names.is_empty()`. Make `apply_action` take `&WorkspaceEdit`, so the dead branch disappears. Pick by index instead of cloning.

### 28. Small issues in `rename/apply.rs`
- **Severity:** Minor
- **Location:** `src/app/rename/apply.rs:455` (`write_closed(&mut self)` never mutates `self`); `136` (`moves_of(..).unwrap_or_default()` swallows the move error in `files_now`); `439-442` and `469-472` (duplicate "nothing" or join formatting); `411` (`as isize` casts)
- **Guideline:** API Design (unneeded `&mut`); Error Handling: hidden errors.
- **Explanation:** `&mut self` falsely signals mutation and blocks shared borrows. A bad edit surfaces the move problem later, but `files_now` then reports files without applying moves.
- **Suggested improvement:** Use `&self`. Add `fn joined_or_nothing(&[&str]) -> String`. Use `isize::try_from`, or compute the delta with `abs_diff` arithmetic.

### 29. Completion resolve blocks the UI thread for up to 2 seconds
- **Severity:** Minor
- **Location:** `src/app/completion.rs:333-363` (`request.wait(RESOLVE_PATIENCE)`)
- **Guideline:** Performance: avoid blocking work on the event loop; Correctness.
- **Explanation:** `Tab` on a suggestion calls `lsp_sync()` and then waits synchronously for the server, from the task that drives the redraw loop. A slow rust-analyzer freezes input and rendering for 2 seconds. The code comments the intent, but the pending-request machinery used by the other features is async.
- **Suggested improvement:** Apply the item immediately and add the resolved import edits as a follow-up undo step when they arrive, or at least lower the patience. This is a UX trade-off, so measure first.

### 30. Go-to-definition list reads whole files synchronously
- **Severity:** Minor
- **Location:** `src/app/definition.rs:168-177`, `286-292` (`line_text`)
- **Guideline:** Performance: unnecessary allocation and I/O.
- **Explanation:** For every location in the list (for example 100 references) the whole file is read with `read_to_string` on the UI thread. A reference list in one large file reads it 100 times.
- **Suggested improvement:** Cache per path within the call (`HashMap<PathBuf, String>`), or read lines with a `BufReader` up to the wanted line. Do this only if it shows up in a measurement.

### 31. Plugin pushes do filesystem work after every input and event
- **Severity:** Minor
- **Location:** `src/app/plugin_files.rs:81-109`, `114-153`; called from `mod.rs:473-474` and `555-556`
- **Guideline:** Performance: repeated work.
- **Explanation:** For every key and event, and for each plugin with `files.text`, `file_key` calls `fs::metadata` just to decide whether to push. `push_plugin_sizes` computes `state.placements(screen)` (a layout pass) each time. `file_message` stats, reads, then serializes up to 1 MB with `to_bytes()` just to size-check and throws the bytes away; the runtime serializes the message again.
- **Suggested improvement:** Do the metadata check at most once per tick or on `FilesChanged`. Compute placements only when the screen size or layout version changed. Cache the serialized length, or let the runtime report `TooBig`.

### 32. `saved_state()` clones the entire saved state for single lookups
- **Severity:** Minor
- **Location:** `src/app/saved.rs:273-276`; used by `plugin_manager.rs:396-398` (`saved_plugin`), `plugins.rs:355` (iteration), `config_files.rs:179`
- **Guideline:** Ownership: unnecessary clones.
- **Explanation:** `saved_plugin(id)` clones the whole `SavedState` (breakpoints, watches and trust list of every project) to read one record, then clones the record again with `.cloned()`. `plugin_rows` reaches this several times per row (`state_of`, `next_for`, `row_of_installed`), so each refresh of the manager list copies the state O(rows) times.
- **Suggested improvement:** Return `&SavedState` (`pub(super) fn saved_state(&self) -> &SavedState`). Make `saved_plugin` return `Option<&PluginRecord>`.

### 33. Magic positional `[bool; 4]` ties `Saved::new` to the order of `PANES`
- **Severity:** Minor
- **Location:** `src/app/mod.rs:317` (`Saved::new([true, false, true, true])`); `saved.rs:221-226`
- **Guideline:** Types: strong types over primitives; Readability.
- **Explanation:** The array is indexed by the order of `PANES` in another file (Explorer, Terminal, StatusBar, MenuBar). Reordering `PANES` silently flips which pane is "shown".
- **Suggested improvement:** Build it from the panes (`PANES.map(|kind| default_shown(&kind))`) or from a struct with named fields.

### 34. Mutable state exposed as `pub(super)` fields
- **Severity:** Minor
- **Location:** `src/app/run.rs:95-116` (`Runs.list`, `active`, `picker_wanted`, `picker`); `plugins.rs:24-41` (`Plugins`, all nine fields); `plugin_manager.rs:21-34` (`found`, `stopped`, `waiting`, `return_to`); `lsp.rs:429-461`
- **Guideline:** API Design: poor encapsulation; Types: avoid public mutable fields.
- **Explanation:** Sibling modules write these fields directly (for example `self.plugins.manager.found = manifest::scan(..)`, `waiting.push_front`), so the invariants (such as "`running` and `actions` stay in step") live nowhere.
- **Suggested improvement:** Methods on `Manager` for the mutations that exist today (`rescan`, `queue_question`).

### 35. `write_open_key` treats an unreadable keymap as empty
- **Severity:** Minor
- **Location:** `src/app/plugin_keys.rs:514`
- **Guideline:** Error Handling: hidden errors.
- **Explanation:** `fs::read_to_string(&path).unwrap_or_default()` also turns "exists but is not UTF-8 or not readable" into `""`, so the code then appends the "# Your keymap" header and the binding to a file it could not read. Only `NotFound` should mean empty.
- **Suggested improvement:** `match fs::read_to_string(&path) { Ok(t) => t, Err(e) if e.kind() == NotFound => String::new(), Err(e) => return Err(e) }`.

### 36. Test layout does not follow either documented convention, and production code carries test hooks
- **Severity:** Minor
- **Location:** `src/tests/*_tests.rs` (81 files) and `src/app/{keys,mouse,watching,background,plugin_files}/tests.rs`, `rename/apply/tests.rs`; test-only items in production files: `app/mod.rs:58-66` (`watches_for_tests`, `whole_lines_for_tests`), `formatting.rs:595-602`, about 20 `#[cfg(test)] fn with_*` builders
- **Guideline:** Testing: tests cover public behaviour; coding guideline says `{name}_tests.rs` next to the source (AGENTS.md says sibling `tests.rs`).
- **Explanation:** Most behaviour of the large modules (`plugin_list`, `debug`, `run`, `plugins`, `code_actions`, `formatting`) is tested only through whole-`App` tests in a separate folder. That is acceptable as integration coverage, but the helper functions with real logic (`whole_lines`, `remap`, `moves_of`, `update_for`, `key_message`, `run_build`) have little or no module-level test beside them, and the project has three different placements. Re-exporting private functions under `#[cfg(test)]` shims (`whole_lines_for_tests`) shows tests reaching into internals.
- **Suggested improvement:** Pick one convention and document it. Move pure-function tests next to their module (`mod tests;` under the file reaches private items without shims). Keep `src/tests/` for App-level scenarios.

---

## Suggestion

### 37. Many unbounded `std::thread::spawn` calls
- **Severity:** Suggestion
- **Location:** `plugin_list.rs:283, 293, 664, 990`; `git.rs:74, 180` (`git_bases_step` spawns one thread per open document on every generation change); `run.rs:274, 526`; `plugins.rs:221`; `popups/root_save.rs:119`
- **Guideline:** Performance (only after measuring); Ownership.
- **Explanation:** There is no join handle, bound or pool. With many open documents, each git status change can start dozens of `git` subprocess threads at once. The spawn sites also each repeat the "clone a sender and blocking_send" boilerplate.
- **Suggested improvement:** A small `Background::spawn(job)` helper with a bounded worker count, and one place for the send. Measure first.

### 38. `handle_input` and `handle_event` repeat overlapping lists of post-hooks
- **Severity:** Suggestion
- **Location:** `src/app/mod.rs:470-481` vs `552-558`
- **Guideline:** Maintainability.
- **Explanation:** `sync_breakpoint_documents`, `debug_sync_breakpoints`, `push_file_text`, `push_plugin_sizes`, `remember_panes` and `remember_recent` are listed in both, in the same order, with other hooks only in one. A new hook easily lands in only one.
- **Suggested improvement:** A private `after_change()` for the common tail.

### 39. `AppState` and `App` are very wide, and `state/popups.rs` is boilerplate
- **Severity:** Suggestion
- **Location:** `src/app/state.rs:60-156` (about 40 fields); `src/app/mod.rs:96-150`; `src/app/state/popups.rs` (469 lines of `open_x`, `x`, `x_mut`, `take_x` per popup variant)
- **Guideline:** API Design: poor encapsulation; Maintainability.
- **Explanation:** Each new popup adds four near-identical methods, and the `take_*` ones use `matches!` followed by a `match` with an unreachable `_ => None` arm. That arm would drop the open popup (`mem::take` has already replaced it) if the invariant broke.
- **Suggested improvement:** A small macro or generic `fn as_mut<T>(&mut self, pick: impl Fn(&mut Popup) -> Option<&mut T>)`. Group related fields (find, hover, debug, plugin) into sub-structs.

### 40. Stringly-typed action payloads
- **Severity:** Suggestion
- **Location:** `plugin_list.rs:1086` and `1102-1109` (`Action::PluginPlaced("id:place")` built with `format!` and parsed with `rsplit_once(':')`); `plugins.rs:348, 381, 422, 604` (`Action::Plugin("plugin:action")`, split on `:`)
- **Guideline:** Types: strong types over primitives.
- **Explanation:** A plugin id containing `:` breaks parsing.
- **Suggested improvement:** `Action::PluginPlaced { id, place }` and `Action::Plugin { plugin, action }`.

### 41. Side effects inside `for_each`, and a take-then-reopen dance
- **Severity:** Suggestion
- **Location:** `src/app/config_files.rs:330-336`
- **Guideline:** Readability: prefer explicit control flow.
- **Explanation:** `"Theme: New from ".chars().for_each(|c| palette.insert(c))` after `open_palette()` then `take_palette()`/`open_palette(palette)` is indirect.
- **Suggested improvement:** `open_palette_with(query)`.

### 42. Placeholder fields in `debug::evaluate::Ask`, and loosely-coupled flags in `hover::Candidate`
- **Severity:** Suggestion
- **Location:** `src/app/debug_hover.rs`/`debug/evaluate.rs:136-144` (refresh builds `Ask { byte: 0, anchor: Position::default(), word: None, selection: true }` with meaningless values); `hover.rs:46-61` (`asked: bool`, `selection: bool` valid only when `expression.is_some()`)
- **Guideline:** Types: make invalid states unrepresentable.
- **Explanation:** A refresh does not have a byte, an anchor or a word, but the type forces fake ones. `selection` only means something with an `expression`.
- **Suggested improvement:** Split `Ask` into `Origin::Pointer { .. }` and `Origin::Refresh`, and group `expression` and `selection` into one `Option<Expression>`.

### 43. A user-visible message hard-codes a limit that lives elsewhere
- **Severity:** Suggestion
- **Location:** `src/app/plugins.rs:80-82` ("failed three times within a minute"); the rule is documented at `src/plugins/mod.rs:13`
- **Guideline:** Maintainability.
- **Explanation:** If the runtime's limit changes, the message becomes wrong.
- **Suggested improvement:** Export the limit constants from `plugins` and format them into the text.

### 44. Direction and step encoded as signed integers
- **Severity:** Suggestion
- **Location:** `terminal_pane.rs:83`, `bars.rs:49`, `popups/search.rs:113`, `keys.rs:332-336, 354-355, 411-412, 458-461`
- **Guideline:** Types: enums over primitives.
- **Explanation:** `isize` parameters whose only meaningful values are -1 and 1 (and `5`, `50` for resizing) are easy to misuse. This overlaps with finding 15 but is lower priority.
- **Suggested improvement:** Fold into the `Direction` enum of finding 15; name the resize deltas as constants.

---

<!-- Part D -->
# Review D: `src/components/` (about 16k non-test lines)

Reviewed against `docs/Guideline PROMPT REVIEW.md` and `docs/Guideline PROMPT CODE.md`. All non-test files were read. `cargo clippy --workspace` was run to confirm the dead-code warnings. Line numbers refer to the current working tree.

## Overall assessment

The code is in good shape and follows the guidelines closely.

- **Unsafe and panics:** there is no `unsafe`, no `unwrap()` and no `panic!` in non-test code. There is exactly one `expect` (workspace/documents.rs:284), and it guards a real internal invariant.
- **Error types:** errors are mostly concrete (`BufferError`, `FileError`, `ExplorerError`, `SearchError`), except for a few String-typed places listed below.
- **Docs, early returns, naming:** doc comments are thorough, early returns and `let ... else` are used consistently, and naming is clear.
- **Tests:** filesystem tests use `tempfile`.

The real problems are these:

1. **Two functional bugs in drawing and scrolling code.** The completion-box highlight is lost while the list scrolls. A pinned debug-hover box does not scroll its selection into view. Neither is covered by a test.
2. **A lot of copy-paste across popups and list views.**
   - Near-identical modules: `action_list.rs` and `definition_list.rs`.
   - A hand-rolled scroll/selection model repeated in about six components.
   - Five word-wrap or truncate helpers.
   - Twenty copies of the popup box chrome.
3. **Weak types.**
   - Anonymous tuples where the element order is inconsistent between sibling functions (terminal cells).
   - Boolean parameters and flags.
   - A few functions with 7 parameters.
4. **A few oversized functions.** `editor/render.rs::draw_lines` is 144 lines, and `plugin_list.rs::render` and `debug/view.rs::render` are about 100 to 140 lines each.
5. **File layout.** The globals, then `pub`, then private ordering is violated in a number of files.

Counts: 2 Critical, 5 Major, 32 Minor, 5 Suggestion.

---

## Critical

### C1. Completion box loses its highlighted row whenever the list scrolls
- **Severity:** Critical
- **Location:** src/components/completion_box.rs:183 and 187 (`rows` / `first_row` at 102 and 126)
- **Guideline:** Correctness; Readability (hidden ordering coupling).
- **Explanation:**
  - `render` reads `first = self.scroll.get()` (line 183) before `self.rows(..)` is called (line 187).
  - `rows()` is the only place that calls `first_row()`, which updates `scroll`.
  - So when the selection moves past the visible window, the `marked` test (`first + offset == selected`, line 191) uses the previous scroll value.
  - Example: with 20 items and `selected` going from 7 to 8, the stale `first` is 0 and the new first is 1. The marked offset would be 8, but only offsets 0 to 7 exist, so no row is highlighted in that frame.
  - This happens on every key press that scrolls the list down. Pressing the key repeatedly shows a list with no visible selection.
  - The integration tests only check `completion().selected_item()` (src/tests/completion_tests/keys.rs:46-52), never the drawn row, so this is not caught.
- **Suggested improvement:**
  - Compute `first = self.first_row(selected)` once at the top of `render` and pass it to `rows`/the marking logic. This removes the hidden "rows() must run first" dependency.
  - Add a render-level test (TestBackend) that scrolls past `MOST_ROWS` and asserts the highlighted style is on screen.

### C2. Pinned debug-hover box: keyboard selection never scrolls into view
- **Severity:** Critical
- **Location:** src/components/debug/hover.rs:177-195 (`select`, `move_by`, `scroll_by`) and 297-314 (`render`); caller src/app/debug_hover.rs:195-224
- **Guideline:** Correctness; edge cases.
- **Explanation:**
  - `move_by` only changes `selected`. `scroll` is changed only by the wheel (`scroll_by`).
  - `render` shows `scroll..scroll+rows` and highlights `index == selected` among the whole list.
  - When the tree has more than `MOST_LINES` (14) rows, for example an expanded Vec, pressing Down past row 14 moves the selection off-screen. Enter, Right and Left then toggle a row the user cannot see.
  - The pane views (`Cursor::keep`, `ErrorsView::prepare`, `Explorer::prepare`) all implement follow-selection. This view is the odd one out.
  - There is no test: `debug/tests.rs` has no scroll test.
- **Suggested improvement:** After `move_by`/`select`, clamp `scroll` so `selected` is inside `scroll..scroll+visible_rows` (the same `keep` logic as `debug::view::Cursor`). Better: reuse one shared cursor type (see Major M4). Add a test with more than 14 rows.

---

## Major

### M1. `action_list.rs` and `definition_list.rs` are copy-paste of each other
- **Severity:** Major
- **Location:** src/components/popups/action_list.rs:24-57, 114-167, 169-196; src/components/popups/definition_list.rs:26-56, 107-151, 153-180
- **Guideline:** Maintainability; "do not invent custom APIs"; composition.
- **Explanation:**
  - These are the same widget:
    - `Command::from_key` is identical apart from the `Pick` variant.
    - `step` / `page` / `select_at` / `contains` are character-for-character the same.
    - The scroll-keeping block in `render` (lines 188-196 and 172-180) is the same, and so is the box geometry.
  - A fix in one (for example the `rows.abs() == 1` "wrap vs clamp" rule) must be made twice.
  - The same `Release || CONTROL|ALT` guard prefix is also repeated in `menu.rs`, `plugin_list.rs`, `plugin_readme.rs`, `palette.rs`, `root_prompt.rs`, `quit_prompt.rs` and `name_prompt.rs`.
- **Suggested improvement:**
  - Extract a generic `PickList<T>` (entries, selected, `Cell` scroll and area, `step`, `page`, `select_at`, `contains`, box geometry). Make `ActionList` and `DefinitionList` thin row-renderers over it.
  - Add a shared `plain_key(key) -> Option<KeyCode>` helper for the guard.

### M2. `draw_lines` is a 144-line function doing five jobs
- **Severity:** Major
- **Location:** src/components/editor/render.rs:180-323
- **Guideline:** Function size / one responsibility; readability (nested control flow).
- **Explanation:** One loop body builds:
  - the gutter (git bar, code-action mark, breakpoint, debugger arrow, as four chained `match` shadowings of `bar`),
  - the diagnostic filter,
  - the selection/search-match styling closure,
  - the inline diagnostic message,
  - the stop-line styling.

  The closure at lines 271-291 contains a guard-match with nested `any` calls. This is hard to follow and hard to test.
- **Suggested improvement:**
  - Extract `gutter_cell(line, ...) -> Span` (the four-way precedence: stop arrow, breakpoint, action, git).
  - Extract `line_spans(...)` for the cells and `inline_diagnostic(...)` for the trailing message.
  - Make the precedence a single `if let` chain.

### M3. Terminal cell coordinates are anonymous tuples with opposite orders
- **Severity:** Major
- **Location:** src/components/terminal/mod.rs:241-245 (`screen_cell` returns `(column, row)`), 249-257 (`clamped_cell` returns `(row, column)`), 47-49 (`Selection.anchor/head` are `(row, column)`), 273 and 289 (`begin_selection(cell: (row, column))`)
- **Guideline:** Types: strong types over primitives and named structs; API design.
- **Explanation:**
  - Two sibling methods with the same signature `-> Option<(u16, u16)>` return the opposite element order. The doc comments are the only protection.
  - The caller in src/app/terminal_pane.rs:296-299 has to swap them by hand (`let (row, column) = (cell.1, cell.0);`). A forgotten swap would silently select the wrong cell.
- **Suggested improvement:** Introduce `struct Cell { row: u16, column: u16 }` (or use `ratatui::layout::Position` consistently), return it from both functions, and store it in `Selection`. Add `From`/`Ord` as needed (the derived ordering for `selection_range` needs `row` first).

### M4. Selection/scroll navigation logic reimplemented at least six times
- **Severity:** Major
- **Location:**
  - src/components/explorer/mod.rs:145-166 and explorer/render.rs:19-34
  - src/components/errors/mod.rs:129-181
  - src/components/debug/view.rs:78-105 (`Cursor`)
  - src/components/debug/hover.rs:177-195
  - src/components/popups/palette.rs:134-140, 201
  - src/components/popups/search/mod.rs:187-201
  - src/components/popups/finder/mod.rs:313-319
  - src/components/popups/quit_prompt.rs:100-106
- **Guideline:** Maintainability; composition; "do not invent custom APIs".
- **Explanation:**
  - The Up/Down/PageUp/PageDown/First/Last arithmetic and the `prepare` "keep selection in view" block are copied with small differences. Examples: `max_scroll` uses `.max(1)` in errors but not in explorer; the debug Hover omits follow entirely (C2).
  - `debug::view::Cursor` is already the right abstraction but is private to that module.
- **Suggested improvement:** Lift `Cursor` (selected, scroll, `apply(count, page)`, `keep(count, height, follow)`, `scroll_by`) into a shared module (for example `components/list_cursor.rs`). Use it in explorer, errors, debug view, debug hover, and the popup lists (and in M1's `PickList`).

### M5. Five hand-written width/wrap/truncate helpers with different semantics
- **Severity:** Major
- **Location:**
  - src/components/popups/diagnostic_popup.rs:269 `wrap_text`
  - src/components/notifications.rs:293 `wrap`, and :329 `split_at_width`
  - src/components/popups/plugin_readme.rs:219 `wrap`
  - src/components/workspace/render.rs:296 `truncate`
  - src/components/terminal/render.rs:101 `truncate` (identical to the workspace one)
  - src/components/editor/render.rs:551 `fit`
  - src/components/popups/name_prompt.rs:410 `head_fitting`
- **Guideline:** Maintainability; do not duplicate; API design (shared helpers in an unrelated module).
- **Explanation:**
  - Three different word-wrap algorithms (they differ on leading spaces, over-long words and trailing spaces) and three width-truncators live in different files.
  - The shared ones sit in unrelated modules: `wrap_text` and `place` live in `diagnostic_popup.rs` but are imported by `hover_box`, `completion_box` and `debug/hover`. `fit` lives in `editor/render.rs`. `short_path` lives in `definition_list.rs` but is imported by `errors/mod.rs` and `debug/view.rs`.
  - Behaviour is already inconsistent (`fit` appends `…`, `truncate` does not).
- **Suggested improvement:** Create `components/text.rs` (or put it in the `ui` crate) with `wrap`, `truncate`, `fit`, `place`, `short_path`, and delete the copies. Test it once.

---

## Minor

### m1. Dead code: `RootPrompt::document` (clippy)
- **Severity:** Minor
- **Location:** src/components/popups/root_prompt.rs:49 (field `document`) and :68 (method `document()`); constructor :58
- **Guideline:** API design (public implementation details); dead code.
- **Explanation:** The field and its getter are never read. `RootPrompt::new` still takes and stores a `DocumentId`.
- **Suggested improvement:** Delete the field and getter and drop the constructor parameter. Update the call site in app. If a later step needs it, add it then.

### m2. Dead code: `DefinitionList::entries` (clippy)
- **Severity:** Minor
- **Location:** src/components/popups/definition_list.rs:91-93
- **Guideline:** Dead code; visibility.
- **Explanation:** `entries()` and `selected()` are flagged as unused in non-test builds (the `selected` warning refers to the getter at :99). `ActionList::entries` is correctly `#[cfg(test)]`.
- **Suggested improvement:** Remove them, or mark them `#[cfg(test)]` like `action_list.rs:100` if only tests use them.

### m3. Functions with more than 6 parameters
- **Severity:** Minor
- **Location:**
  - src/components/debug/hover.rs:105-113 (`with`, 7 parameters)
  - src/components/errors/mod.rs:49-57 (`ErrorRow::new`, 7 parameters)
  - Borderline: src/components/popups/confirm.rs:188-195 (`install_from_source`: six `&str` parameters that are easy to transpose), :213-220 (`update_plugin`: six parameters including a `bool`), src/components/hover_box.rs:57-64 (`HoverBox::new`: six parameters including `pinned: bool`), src/components/debug/hover.rs:64-71 and :86-93 (`valued`/`refused`)
- **Guideline:** Functions: avoid excessive parameters (more than 6); strong types.
- **Explanation:** Several of these share the trio `(anchor, document, version, pinned)`.
- **Suggested improvement:**
  - Introduce small parameter structs, for example `HoverContext { anchor, document, version }` and `PluginInstall { id, name, version, label, url, sha256 }`.
  - `ErrorRow::new` could take `(DocumentId, FileLabel, Position, &Diagnostic)`.

### m4. Needless `clone()` to satisfy the borrow checker
- **Severity:** Minor
- **Location:** src/components/debug/hover.rs:224 (`self.tree.set_root(&self.expression.clone(), root)`)
- **Guideline:** Ownership: "Never clone just to satisfy the borrow checker".
- **Explanation:** `tree` and `expression` are disjoint fields, so `self.tree.set_root(&self.expression, root)` compiles without the clone.
- **Suggested improvement:** Drop the `.clone()`.

### m5. Boolean parameters and flags where enums read better
- **Severity:** Minor
- **Location:**
  - mouse_press `extend: bool`: src/components/editor/mouse_ops.rs:50; src/components/workspace/mouse.rs:15
  - `edit_each_placing(.., stay: bool, ..)`: src/components/editor/multi.rs:257-261
  - `Content { focused, live }`: src/components/editor/render.rs:63-66
  - `PluginList::step(forwards: bool)`: src/components/popups/plugin_list.rs:207
  - `Find::step_from(.., forward: bool)`: src/components/popups/find/mod.rs:392
  - `draw_empty(.., focused: bool)`: src/components/workspace/render.rs:179
  - `render_view(.., typing, has_keys)` and `render_part(.., typing, has_keys)`: src/components/render.rs:245-246, 259-260
  - `DiskEvent::ReadOnly(String, bool)`: src/components/workspace/disk.rs:23
  - `Menu { keeps_focus, from_anchor_row }`: src/components/popups/menu.rs:107-109
  - `Note.warn`, `Row.grey`: src/components/popups/plugin_list.rs:144, 152
  - `Shown { selected, active, drop, focused }` (four bools): src/components/explorer/render.rs:108-119
- **Guideline:** Types: enums over booleans; Functions: avoid boolean parameters.
- **Explanation:** Call sites like `edit_each_placing(None, true, ...)` are unreadable.
- **Suggested improvement:** Use `Extend::{Replace, Extend}`, `CursorPlacement::{AfterChange, Stay}`, `Direction::{Forward, Back}`, `ReadOnly::{Now, NoLonger}`, a `Focus::{None, Pane, Typing}` for `typing`/`has_keys`, and so on.

### m6. Anonymous tuples as return types, fields and variants
- **Severity:** Minor
- **Location:**
  - src/components/workspace/documents.rs:124-127 (`root_save_source -> (PathBuf, String, u64, bool)`) and :213-216 (`document_info -> (Option<&Path>, String, &Buffer)`)
  - src/components/workspace/panes.rs:104-108 (`nearest_pane -> (PaneId, (u32, u32))`)
  - src/components/workspace/mod.rs:110 (`overflow: (Rect, Rect)`), :112 (`hidden: (usize, usize)`), :144 (`empty: (Buffer, ViewState)`)
  - src/components/popups/find/mod.rs:349 (`match_count -> (usize, bool)`) and :355 (`line_replacements -> Vec<(usize, usize, String)>`)
  - src/components/popups/search/mod.rs:145 (`results -> (&[Hit], usize)`)
  - src/components/popups/finder/mod.rs:160 (`place: Option<(usize, Option<usize>)>`, though `target::Parsed` already has named fields)
  - src/components/popups/name_prompt.rs:427 (`parse_place -> Option<(usize, Option<usize>)>`)
  - src/components/debug/model.rs:96 (`Read.page: Option<(i64, i64)>`)
  - src/components/debug/view.rs:361 (`layout -> (Rect, Rect, Rect, Rect)`)
  - src/components/editor/view_state.rs:23 (`seen_view: Option<(Position, u64, Rect)>`)
  - src/components/popups/confirm.rs:237 (`update_all(&[(String, String, String)])`)
- **Guideline:** Types: named structs over tuples/primitives.
- **Explanation:** Positional fields of the same type (`u64, bool`, `Rect, Rect`) are easy to mix up, and the order is documented only in prose.
- **Suggested improvement:** Name them: `RootSaveSource`, `DocumentInfo`, `PaneBounds`, `MatchCount { count, capped }`, `PageRequest { start, count }`, `DebugAreas`, `PluginUpdate { name, from, to }`, and so on. Reuse `Parsed` / `Place` instead of `(usize, Option<usize>)`.

### m7. `String` errors and stringly-typed results
- **Severity:** Minor
- **Location:**
  - src/components/popups/find/mod.rs:581 (`build(..) -> Result<Regex, String>`), :108-111 (`Problem::InvalidPattern(String)`)
  - src/components/popups/search/worker.rs:31-35 (`SearchError::Pattern(String)`; the `thiserror` enum just wraps a pre-formatted string)
  - src/components/workspace/disk.rs:24-25 (`DiskEvent::Unreadable(String)` is filled from `error.to_string()` at :92 and :112)
  - src/components/workspace/bulk.rs:53-54 (`SaveAll.failed: Vec<(String, String)>`)
  - src/components/workspace/disk.rs:297-299 (`FileDeleted` message is rebuilt as a literal in bulk.rs:197-200)
- **Guideline:** Error handling: generic String errors; suggest concrete error types.
- **Explanation:** The structured regex/`FileError` is flattened to text at the lowest layer, so callers cannot match on the cause. `short_message` is duplicated verbatim in find/mod.rs:600 and search/worker.rs:90.
- **Suggested improvement:**
  - Define `PatternError { message }` (or keep `regex::Error`/`grep_regex::Error` as `#[source]`) in one shared place.
  - Add a single `pattern::short_message`.
  - Carry `FileError` in `DiskEvent::Unreadable` and `SaveAll.failed`, and format at the UI edge.

### m8. Duplicate methods/helpers in the workspace and explorer modules
- **Severity:** Minor
- **Location:**
  - `document_paths` (src/components/workspace/git_lines.rs:28-33) is identical to `documents_with_paths` (src/components/workspace/documents.rs:205-210).
  - Three copies of "find (pane, tab) by predicate": bulk.rs:130 (`find_view`), tabs.rs:40 (`find_tab_of`), and the inline closure at editing.rs:57-60, which is identical to `find_tab_of`.
  - `context_press` (mouse.rs:79-94) and `place_cursor_at` (mouse.rs:116-131) have the same 8-line prologue.
  - Explorer `click` (explorer/mod.rs:206-213) and `select_at_or_root` (:223-227) duplicate `drag.rs::row_at` (:160-166).
  - `Press`/`Drag` creation is duplicated at drag.rs:85-88 and :105-108.
  - `Find::retarget` (find/mod.rs:384-387) and `Find::continue_from` (:377-380) have identical bodies.
  - Walk setup, `HIDDEN_DIRECTORY` and `is_file`: explorer/tree.rs:14, finder/scan.rs:18 and :92-97, search/worker.rs:29 and :214-219. The `WalkBuilder::new(root).hidden(false).require_git(false).filter_entry(..)` chain also appears in scan.rs:44 and worker.rs:106.
- **Guideline:** Maintainability; readability.
- **Suggested improvement:** Delete the duplicates and call one implementation. Put the walker/`is_file` in one `files` helper.

### m9. Tab labels built twice, title computed twice per frame
- **Severity:** Minor
- **Location:** src/components/workspace/render.rs:53-57 (draw) and :124-125 (`layout_pane`)
- **Guideline:** Ownership/allocations; duplication.
- **Explanation:** `format!(" {pin}{} ", self.tab_title(..))` is built in `render` and again in `layout_pane` (which then discards the strings after measuring). Each frame does it twice for every tab. A new tab-label change has to be made in both places.
- **Suggested improvement:** Build the labels once in `prepare` (store `Vec<String>` in `Pane`) and reuse them in `render`.

### m10. `with_tab_editor`: repeated `if !log.is_empty()` blocks
- **Severity:** Minor
- **Location:** src/components/workspace/editing.rs:88-90, :106-108, :109-111
- **Guideline:** Readability.
- **Explanation:** Three separate checks of `log.is_empty()` (two adjacent and identical). The function is about 50 lines and interleaves lending buffer/view, replaying, event logging and syntax bookkeeping.
- **Suggested improvement:** Early-return structure or a `let edited = !log.is_empty();` with a single `if edited { .. }` block. Consider splitting "lend" and "record edits" into two helpers.

### m11. `check_document` drops a read-only change when content also changed; reload errors repeat
- **Severity:** Minor
- **Location:** src/components/workspace/disk.rs:78-85 (`self.check_content(id).or(permissions)`) and :110-113
- **Guideline:** Correctness; hidden errors.
- **Explanation:**
  - `doc.read_only` is updated before `check_content`. If the content check also returns an event, the `ReadOnly` event is silently dropped and the tab note and user message are never updated.
  - When an unmodified document fails to reload, `reload_document` returns `Err` before `warned` is set, so every later file event or `check_all_disk` repeats `Unreadable`.
- **Suggested improvement:** Return both events (`Vec<DiskEvent>` or `SmallVec`), or record `warned` for the failed reload too. If the repeat is intended, document it.

### m12. Repeated `self.documents[&id]` indexing next to a documented `expect`
- **Severity:** Minor
- **Location:** 12 sites: workspace/render.rs:65, 79, 110, 168; highlights.rs:29, 60; tabs.rs:35, 90, 137; bulk.rs:121, 142; editing.rs:14. Compare documents.rs:281-285 (`document_mut`, one `expect` with a message).
- **Guideline:** Error handling: unwrap/expect/panic only for internal invariants, and stated once.
- **Explanation:** `HashMap` index panics are the same invariant ("every tab refers to an open document") but with no message and spread over 12 sites. Only the `&mut` variant has an accessor.
- **Suggested improvement:** Add `fn document(&self, id: DocumentId) -> &Document` with one `expect("every tab refers to an open document")` next to `document_mut`, and use it everywhere.

### m13. File layout: globals, then pub, then private, is violated in many files
- **Severity:** Minor
- **Location:**
  - src/components/completion_box.rs:232-237 (`DOCS_*` consts after the `impl`), :326 (`MIN_DOCS_ROWS` const after its use)
  - src/components/editor/render.rs:28-41 (const, fn and const interleaved), :521-523 (`INLINE_*` consts mid-file after their first user)
  - src/components/workspace/mod.rs:66-116 (private `Document`/`Tab`/`Pane` before `pub Workspace`), :203 (`contains` last)
  - src/components/workspace/render.rs:104 (`PIN` const mid-file), src/components/workspace/bulk.rs:211-213 (`REMEMBERED_FILES` mid-file)
  - src/components/workspace/tree.rs:20-24 (`HALF/LEAST/MOST` after the enum)
  - src/components/terminal/mod.rs:486-487 (`ADD_BUTTON_WIDTH` after its user)
  - src/components/hover_box.rs:31-37 (private `Kind` before the pub struct)
  - src/components/debug/hover.rs:31-38 (private `Body` before `DebugHoverBox`)
  - src/components/popups/quit_prompt.rs:72-76 (the `Render` impl wedged between the struct and its inherent impl)
- **Guideline:** Style: file layout globals first, then `pub` items, then private items.
- **Suggested improvement:** Move the constants to the top of each file, `pub(crate)` items before private helper types and fns.

### m14. Misplaced or wrong doc comments and a user-visible typo
- **Severity:** Minor
- **Location:**
  - src/components/editor/render.rs:26-28 ("Smallest line-number column, in digits." sits above `STOP_ARROW`; it belongs to `MIN_GUTTER_DIGITS` at :41)
  - src/components/explorer/mod.rs:132-134 (the doc "Whether a screen cell is inside the explorer." is on `area()`; `contains` at :138 has none)
  - src/components/terminal/render.rs:13-14 (says "the tab bar when there is more than one tab", but the bar is drawn whenever `tab_bar.height > 0`, i.e. with one tab)
  - src/components/status_view.rs:36 (user-visible text "quit: Ctr+Q, commandPallet: f1": typo "Ctr", and "commandPallet" misspells "Palette")
  - src/components/popups/plugin_list.rs:3 (a single doc line runs past 100 columns and mixes the key list with prose)
- **Guideline:** Readability; naming.
- **Suggested improvement:** Fix the comments and the label text.

### m15. Popup box chrome and input-line rendering repeated about 20 times
- **Severity:** Minor
- **Location:**
  - `Block::new().borders(ALL).border_type(Rounded).border_style(..)`, then `backdrop::clear`, then `render_widget(block)`, is repeated in 20 places. Examples: action_list.rs:200-207, definition_list.rs:183-190, root_prompt.rs:132-139, quit_prompt.rs:119-127, palette.rs:160-167, confirm.rs:333-340, key_capture.rs:65-75, finder/mod.rs:363-370, search/mod.rs:243-250, name_prompt.rs:369-376, hover_box.rs:177-186, completion_box.rs:175-181, diagnostic_popup.rs:172-179, debug/hover.rs:285-291, notifications.rs:271-280.
  - The `prompt.width().min(..) as u16` plus `set_cursor_position` input line is repeated in palette.rs:172-179, finder/mod.rs:375-382, search/mod.rs:256-263, name_prompt.rs:381-391, find/mod.rs:514-541.
  - The centering code `x: screen.x + (screen.width - width) / 2` is copied about 12 times.
  - `Rect { x: inner.x + 1, width: inner.width - 2, ..inner }` is copied 5 times.
- **Guideline:** Maintainability; do not duplicate.
- **Suggested improvement:** Add `popup_box(frame, theme, area, title, focused) -> Rect` (inner area), `centered(screen, w, h, top_fraction)`, and a small `InputLine` widget. This also removes most `as u16` casts.

### m16. `confirm.rs` and `key_capture.rs`: box height is estimated, so wrapped text can be clipped
- **Severity:** Minor
- **Location:** src/components/popups/confirm.rs:321-326 and src/components/popups/key_capture.rs:58
- **Guideline:** Correctness (edge cases).
- **Explanation:**
  - `confirm::render` estimates the wrapped height with `line.chars().count().div_ceil(inner_width)`. ratatui's `Wrap { trim: false }` breaks at word boundaries, so a long paragraph (for example the install-from-source warning) takes more rows than the estimate and its tail is cut off. It also counts chars, not display width.
  - `key_capture.rs:58` sets the height to `lines.len() + 4` while it enables `Wrap`. A long `problem` message wraps and is clipped.
- **Suggested improvement:** Use `Paragraph::line_count(width)` (or the shared `wrap`) to compute the height, and measure with `UnicodeWidthStr`.

### m17. Popup `Menu` caps its height to the screen but cannot scroll
- **Severity:** Minor
- **Location:** src/components/popups/menu.rs:390 (`height = (entries.len() + 2).min(screen.height)`), :340-367 (renders every entry), :262 (click maps `position.y - inner.y` straight to the entry index)
- **Guideline:** Correctness (edge cases).
- **Explanation:** On a short terminal, the main menu (many entries) is clipped. The selection can move to an entry that is not drawn and clicks index into hidden entries.
- **Suggested improvement:** Reuse the scroll logic of the list popups (M1/M4), or at least scroll the visible window with the selection.

### m18. Interior mutability (`Cell`/`RefCell`) for draw-time state where `&mut` prepare is available
- **Severity:** Minor
- **Location:** completion_box.rs:44, 49; hover_box.rs (via `TextScroll`); action_list.rs:76, 78; definition_list.rs:75, 77; diagnostic_popup.rs:88-92; plugin_readme.rs:72, 74; debug/hover.rs:59; scroll_text.rs:11-18. Compare components/render.rs:54-60, where `menu.place(screen)` and `name_prompt.place(screen)` already take `&mut`.
- **Guideline:** Ownership: avoid interior mutability unless required.
- **Explanation:**
  - `prepare_and_render` already has `&mut AppState` and mutates other popups before drawing. The `Cell` fields exist only because these `render` methods take `&self`.
  - `scroll_text::TextScroll` was built for this, yet `DiagnosticPopup` (scroll/max_scroll/page Cells) and `Readme` (max_scroll/page) each re-implement the same three numbers by hand.
  - `CompletionBox::rows` mutates `scroll` as a side effect of a "getter" (the cause of C1).
- **Suggested improvement:** Add a `place(&mut self, screen)` step for these popups like `Menu`/`NamePrompt`, and keep `render(&self)` pure. Replace the hand-rolled Cell trios with the shared `TextScroll`/cursor type.

### m19. `Editor`: duplicated byte-range, delete, newline-indent and transaction code
- **Severity:** Minor
- **Location:**
  - `selection_bytes` (editor/mod.rs:428-433) vs `bytes_of` (editor/multi.rs:205-209), which differ only by taking the selection as an argument.
  - Delete-range logic: `delete_backward`/`delete_forward` (mod.rs:294-334) vs `Editor::delete_range` (multi.rs:212-243).
  - Newline indent computation: indent.rs:233-237 vs multi.rs:90-94.
  - The begin_transaction, apply-reversed, commit, `merge_last_two_steps` sequence appears in mod.rs:461-482, multi.rs:257-303, indent.rs:296-309 and replace.rs:57-76.
  - `Editor::move_cursor` (mod.rs:196-207) and `select` (:225-241) call `moved`/`target` twice for the primary selection (once in the loop, again after it).
- **Guideline:** Maintainability; ownership (extra work); readability.
- **Suggested improvement:** Have `selection_bytes` call `bytes_of(primary)`. Express single-cursor delete as the multi-cursor path with one selection. Extract `transaction(edits, after, kind)`. Compute the primary result once by index.

### m20. `view.last_edit = None` set by hand in 19 places while `ViewState::end_edit_run` exists
- **Severity:** Minor
- **Location:** src/components/editor/mod.rs:197, 226, 245, 252, 260, 373, 381; mouse_ops.rs:63, 93, 117, 181, 197; multi.rs:36, 44, 66; indent.rs:301; replace.rs:66; and others. Defined at editor/view_state.rs:116.
- **Guideline:** API design (encapsulation); `ViewState` fields are `pub(super)` and mutated everywhere.
- **Explanation:** The invariant "any non-typing action ends the undo run" is spread over 19 call sites. A new action that forgets it silently merges undo steps.
- **Suggested improvement:** Call `end_edit_run()` from one place (for example a `fn begin_action(&mut self)`), or make `last_edit` private to `ViewState` and drive it from a single `record(kind)`.

### m21. `move_lines(direction: isize)` accepts values it does not handle
- **Severity:** Minor
- **Location:** src/components/editor/move_line.rs:13-23, 30-39
- **Guideline:** Types: enums over primitives; correctness.
- **Explanation:** Only `-1` and `1` are meaningful. For `0` the code takes the "down" branch (`direction > 0` is false but `direction < 0` is false too), so it pops the last line of the block and reinserts it at the top, silently rotating a multi-line selection. Any other value (for example 2) moves the selection by that amount but swaps lines as for `1`, so text and selection disagree. `indent.rs` already has a `Direction` enum for a similar choice.
- **Suggested improvement:** `enum LineMove { Up, Down }`.

### m22. `debug/model.rs`: repeated row construction and range scans
- **Severity:** Minor
- **Location:**
  - `VarRow { .. }` with 11 fields is spelled out 6 times: model.rs:140, 160, 177, 203, 365, 384. `set_root` (157-172) and `add_root` (175-189) are identical apart from `rows.clear()`.
  - The "end of subtree" scan appears three times: collapse (335-339), `collapse_children` (414-418), `end_of_children` (423-426).
  - `reference: i64` uses 0 as the "no children" sentinel (see `has_children`), and the ids and counts are signed (`PAGE: i64`, `total`, `loaded`) with `i64::try_from(..).unwrap_or(0)` hiding conversion failures (:378, 437).
  - `DebugData` exposes all fields as `pub(crate)` mutable (:454-466) while `VarRow.total` is private, which is inconsistent.
- **Guideline:** Maintainability; Types (Option over sentinel); "avoid public mutable fields".
- **Suggested improvement:** Add `VarRow::from_incoming(id, depth, name, &Incoming)`, `fn subtree_end(&self, at)`, `reference: Option<NonZeroI64>` (or a `Children` enum), and accessors on `DebugData`.

### m23. `debug/view.rs`: three near-identical list renders and three `(cursor, count, body)` dispatches
- **Severity:** Minor
- **Location:** src/components/debug/view.rs:168-174 (`apply`), :186-192 (`scroll_by`), :205-228 (`select_at`), :256-356 (`render`, about 100 lines with three copies of the "map rows to lines, style the selected one" block)
- **Guideline:** Function size; duplication.
- **Suggested improvement:** Give `DebugView` a `section_mut(Section) -> (&mut Cursor, count, body)` helper and a `list_lines(...)` function parameterised by the row-to-line mapping.

### m24. Anonymous `Content` and `Overlays` structs duplicate seven fields
- **Severity:** Minor
- **Location:** src/components/editor/render.rs:57-78 (`Content`), :88-99 (destructured), :107-115 (rebuilt as `Overlays`), :170-178
- **Guideline:** Maintainability; composition.
- **Explanation:** `draw` destructures `Content` only to re-pack seven of its fields into `Overlays` with the same names. Adding a new overlay means editing three places. `std::ptr::eq(selection, view.selections().primary())` at :118 identifies the primary cursor by pointer identity instead of an index (fragile if `iter()` ever yields copies).
- **Suggested improvement:** Make `Overlays` a field of `Content` (`pub(crate) overlays: Overlays<'a>`) and drop the repack. Compare the index (`primary_index()`) instead of pointers.

### m25. Other layout/semantic nits in `editor/render.rs` and `completion_box.rs`
- **Severity:** Minor
- **Location:**
  - src/components/editor/render.rs:212-215 (for each visible line, filters the whole diagnostics slice: O(lines × diagnostics) per frame; the slice is documented as ordered by text, so `partition_point` would work), :153 (`(digits(..) + 1) as u16`)
  - src/components/completion_box.rs:102-123 (`rows()` returns `Vec<(String, String, String)>`, with a useless `.enumerate()` then `.map(|(_, item)|` at :107-110, and a `match room > 3 { true => .., false => .. }` on a bool at :116), :211 (`rows as u16`)
- **Guideline:** Types (named struct for the row); readability; performance (only if measured: a large diagnostics list in a big file).
- **Suggested improvement:** Return `Vec<Row { tag, name, note }>`, drop `.enumerate()`, use `if`. Use `partition_point` for diagnostics if profiling shows it matters.

### m26. Unchecked `as` casts despite `try_from` used elsewhere
- **Severity:** Minor
- **Location (60 `as` casts in non-test code); examples:**
  - `as u16` truncation: quit_prompt.rs:129, 169, 171; confirm.rs:326; key_capture.rs:58; palette.rs:175, 240; plugin_list.rs:224-225; finder/mod.rs:378, 435; search/mod.rs:259, 328; notifications.rs:205, 254; terminal/render.rs:175, 183; diagnostic_popup.rs:162; editor/render.rs:153.
  - `as isize`/`as usize` signed round trips: editor/comment.rs:304-310 (`delta` arithmetic with `.max(0) as usize`), editor/multi.rs:307-342 (`growth`, `after_change`, `moved_by`), editor/mouse_ops.rs:205-222, diagnostic_popup.rs:109-115, workspace/tabs.rs:226-230, popups/plugin_readme.rs:118.
- **Guideline:** Types; correctness (silent truncation or clamping hides bugs); consistency (other code uses `u16::try_from(..).unwrap_or(u16::MAX)`).
- **Explanation:** `quit_prompt::area`, for example, does `self.items.len() as u16 * ROWS_PER_ITEM` which can overflow-panic in debug builds and truncate in release for absurd lists. `.max(0) as usize` in the delta code swallows an offset bug instead of surfacing it.
- **Suggested improvement:** Use `u16::try_from(..).unwrap_or(u16::MAX)` (or a `saturating_u16` helper) everywhere. Use `usize::checked_add_signed`/`saturating_add_signed` instead of the signed round trip.

### m27. Cross-component dependencies through the wrong modules
- **Severity:** Minor
- **Location:**
  - errors/mod.rs:11 and debug/view.rs:14 import `short_path` from `popups::definition_list`.
  - hover_box.rs:17, completion_box.rs:17 and debug/hover.rs:21 import `place`/`wrap_text` from `popups::diagnostic_popup`.
  - hover_box.rs:16 and debug/view.rs:13 import `fit`/`slot_of` from `editor::render`.
- **Guideline:** API design (public implementation details; excessive visibility); module cohesion.
- **Explanation:** Utility functions are `pub(crate)` in feature modules, so unrelated components depend on `definition_list` and `diagnostic_popup`. Moving or deleting a popup (as m1/m2 suggest) now ripples into the Errors and Debug panes.
- **Suggested improvement:** Move them to the shared `components/text.rs` / `components/geometry.rs` (see M5, m15).

### m28. Missing tests for the changed behaviour above
- **Severity:** Minor
- **Location:** src/components/completion_box.rs (no unit test; integration tests in src/tests/completion_tests/*.rs assert `completion().selected_item()`, not the drawn marker), src/components/hover_box.rs, src/components/scroll_text.rs, src/components/notifications.rs (`wrap`, `split_at_width`, `evict_one` are private and only reached through `src/tests/notification_tests.rs`), debug hover (src/tests/debug_hover_tests.rs has no case with more than 14 rows).
- **Guideline:** Testing: tests cover public behaviour and edge cases.
- **Explanation:** Both Critical bugs above are in code that has logical-state tests but no draw-level assertion.
- **Suggested improvement:** Add `TestBackend` render tests for completion scrolling, hover-box scrolling and pinned debug hover with a long tree. Add unit tests for `wrap_text`/`wrap` edge cases (empty text, a word wider than the box, wide characters, a lone space).

### m29. `plugin_list.rs`: public fields, 140-line `render`
- **Severity:** Minor
- **Location:** src/components/popups/plugin_list.rs:132-153 (`Row`, `Note` have `pub` fields in a `pub(crate)` module), :215-354 (`render` is 140 lines: size computation, block, list, scroll marks, rule, detail, status, help)
- **Guideline:** Visibility ("never use `pub` unless required"; avoid public mutable fields); function size.
- **Suggested improvement:** Use `pub(crate)` fields (or constructors), and split `render` into `layout`, `draw_rows`, `draw_footer`.

### m30. `plugin_readme.rs`: scrolling by `isize::MIN / 2`
- **Severity:** Minor
- **Location:** src/components/popups/plugin_readme.rs:114-119
- **Guideline:** Readability ("clever code").
- **Explanation:** `Top`/`Bottom` are expressed as huge offsets (`isize::MIN / 2`, `isize::MAX / 2`) so that the later clamp handles them, with a signed round trip `(self.scroll as isize).saturating_add(by).max(0) as usize`.
- **Suggested improvement:** `Command::Top => self.scroll = 0`, `Bottom => self.scroll = max_scroll`, and use `saturating_add_signed` for relative moves.

### m31. `IndentStyle::Spaces(usize)` can hold 0, which the callers divide by
- **Severity:** Minor
- **Location:** src/components/editor/indent.rs:24 (variant), :121 (`width - selection.start().column % width`), :282 (`spaces % width`)
- **Guideline:** Types: strong types over primitives.
- **Explanation:** `TabWidth` (≥1) already exists and is used by `IndentPrefs`, but `IndentStyle` stores a raw `usize`. `Spaces(0)` would panic with a division by zero. Today only `detect` (2..=4) and `TabWidth` feed it, so this is latent.
- **Suggested improvement:** `Spaces(TabWidth)` (or a `NonZeroUsize`).

### m32. `workspace/bulk.rs` mixes unrelated responsibilities
- **Severity:** Minor
- **Location:** src/components/workspace/bulk.rs:211-256 (recent-files list and `open_path_in_split` in a module documented as "Closing several tabs at once... and saving every modified file"); similarly `active_document_id` at disk.rs:305-310 and `LspDocument`/`lsp_documents` at disk.rs:272-303 inside a module about file sync.
- **Guideline:** Maintainability; cohesion; layout.
- **Suggested improvement:** Move `remember_recent`/`recent_paths`/`open_path_in_split` to `documents.rs`, `active_document_id` to `tabs.rs`, `LspDocument` to its own file.

---

## Suggestion

### s1. `canonicalize` (filesystem) calls on the draw path and per lookup
- **Severity:** Suggestion
- **Location:**
  - src/components/explorer/render.rs:93 (`canonical(spelled)` for every frame)
  - src/components/debug/model.rs:504 (`stop_line` canonicalises the pane's path every frame for each pane showing another file while stopped; called from workspace/render.rs:92)
  - src/components/workspace/documents.rs:114-120, 44-57 (`document_of_path`/`documents_with_suffix` canonicalise every open document on each call)
- **Guideline:** Performance (unnecessary work, only after measuring).
- **Explanation:** Each is a syscall per item per call, on the render path or in loops over all documents. `check_disk` (disk.rs:44-76) already shows the codebase knows how to avoid this.
- **Suggested improvement:** Cache the canonical form per document/explorer root (invalidate on `set_path`/root change). Measure before changing.

### s2. `Finder::refilter` allocates the full index list on each backspace
- **Severity:** Suggestion
- **Location:** src/components/popups/finder/mod.rs:327-330
- **Guideline:** Performance/allocations.
- **Explanation:** `FromAllFiles` builds `(0..self.files.len()).collect::<Vec<_>>()` just to iterate it once.
- **Suggested improvement:** Iterate `0..len` directly via an `impl Iterator` or a small enum. Only worth it if the project has very large file lists.

### s3. `Editor::insert_newline` is a pure alias
- **Severity:** Suggestion
- **Location:** src/components/editor/mod.rs:288-290
- **Guideline:** Do not invent custom APIs.
- **Explanation:** `insert_newline` only calls `insert_newline_indented`. Two names for the same operation make call sites inconsistent.
- **Suggested improvement:** Keep one name.

### s4. `closed` tabs, `forgotten`, `recent`: `Vec` used as a bounded queue with `drain`/`insert(0)`
- **Severity:** Suggestion
- **Location:** src/components/workspace/bulk.rs:145-152 and :216-220 (`recent.insert(0, ..)`)
- **Guideline:** Performance/collections.
- **Explanation:** `Vec::insert(0, ..)` and `drain(..extra)` on 20-element lists is fine today; a `VecDeque` states the intent. Keep as is unless the limits grow.
- **Suggested improvement:** Optional: `VecDeque` with `push_front`/`pop_back`.

### s5. Test-only accessors in production files with inconsistent naming
- **Severity:** Suggestion
- **Location:** workspace/tabs.rs:18, 30, 86 (`tab_count_for_test`, `tab_names`, `tab_names_for_test`: two nearly identical `#[cfg(test)]` name helpers), popups/menu.rs:168 (`labels_for_test`), explorer/mod.rs:128 (`body_for_tests`)
- **Guideline:** API design; testing.
- **Explanation:** There are two test helpers (`tab_names` and `tab_names_for_test`) that compute almost the same list, and mixed `_for_test`/`_for_tests`/no-suffix naming.
- **Suggested improvement:** Keep one, with a single naming scheme, or move the helpers into the sibling test modules via `pub(super)` access.

---

<!-- Part E -->
# Review E: src/ outside src/app and src/components

Scope: src/plugins, src/git, src/terminal, src/event, src/settings, src/config, src/keymap, src/persist, src/cli.rs, error.rs, lib.rs, main.rs, startup.rs, plus the test suites in src/tests, src/plugins/tests*, src/git/tests.rs.
Reviewed against `docs/Guideline PROMPT REVIEW.md` and `docs/Guideline PROMPT CODE.md`. Read-only; nothing was edited. `cargo check` could not be run in this sandbox (a glibc symbol-lookup error in build scripts), so the dead-code items below come from reading the code and from the clippy warnings you listed.

## Overall assessment

The code is in good shape. Doc comments explain intent, errors are mostly concrete `thiserror` enums, and the core logic is pure and separable. `git/schedule.rs`, `git/diff.rs` (with its bounded `MAX_EDITS`), `plugins/calls.rs`, `plugins/runs.rs` and `keymap/` are clean and well tested. Every `unsafe` block has a SAFETY comment. Production `unwrap()` is essentially absent, and the `expect()` uses are limited to mutex locks, thread spawn and the embedded default config.

The most important problems are:
- a plugin can exhaust host memory through `host.send` (Major);
- the process-group kill uses a PID that may already be reaped (Major, with a wrong SAFETY comment);
- a corrupt `state.json` is silently replaced on the next save, losing breakpoints, watches and trust decisions (Major);
- there are two dead event/error variants, and a blanket `allow(dead_code)` that hides more of them (Minor).

The rest is mostly file layout, boolean parameters, stringly errors, duplicated helpers and several flaky or environment-dependent tests.

## Major

### M1. A plugin can allocate unbounded host memory through `host.send`
- Severity: Major
- Location: src/plugins/instance.rs:114-131 (`send`), 175 (`std::mem::take(&mut ...sent)`)
- Guideline: Correctness (invalid assumptions); Ownership / Performance (unbounded allocation).
- Explanation: Each `host.send` call checks `len <= MAX_MESSAGE` (4 MiB) and then pushes a fresh `vec![0; len]` into `Host::sent`. The number of messages per call is bounded only by fuel (`CALLBACK_FUEL`, 5M, and up to 2.2G for own-thread plugins). A host call costs only the few wasm instructions around it, and the `memory.read` copy is not charged. A loop calling `send(ptr, 4 MiB)` can therefore queue hundreds of thousands of 4 MiB buffers before fuel runs out, which would exhaust host memory and abort the editor. The memory limit applies only to the guest's linear memory, not to host-side buffers. The stated design is that a hostile or buggy plugin cannot take the editor down.
- Suggested improvement: Cap the number of messages and the total bytes queued per `deliver` call (for example `MAX_MESSAGE` in total, or a `MAX_MESSAGES_PER_CALL`). Return a `wasmi::Error` from `send` to trap the plugin once exceeded. Alternatively, charge fuel in `send` proportional to `len`.

### M2. Process-group kill after the child has been reaped; SAFETY comment is false for that path
- Severity: Major
- Location: src/plugins/exec.rs:178-190 (`stop`) and 197-213 (`signal_group`); compare src/plugins/exec.rs:182-186
- Guideline: Unsafe (SAFETY comment must be true and the invariants preserved); Correctness.
- Explanation: The SAFETY comment says "The child has not been waited for, so its id is still its own." In `stop`, the first branch (`child.try_wait()` returns `Some(status)`) has already reaped the child. It then calls `signal_group(child, Signal::Kill)`, which runs `killpg(pid, SIGKILL)` on a PID that has been released. If no member of that group is left, the PID can be recycled by an unrelated process or group within the window, and the editor kills it. The window is small and the group usually still has members, but it is exactly the case the comment claims cannot happen. `supervise` also never kills the group when `wait` sees the child end normally, which the module docs imply happens ("killed with everything they started when ... lost").
- Suggested improvement: Kill the group before reaping, or only when `try_wait` returned `None`. If leftover grandchildren must be killed after exit, send the signal before `wait`/`try_wait` collects the status, or use a pidfd. Update the SAFETY comment to state the real invariant.

### M3. A corrupt or unreadable `state.json` is replaced by the next save, losing the user's data
- Severity: Major
- Location: src/persist/saved_state.rs:326-352 (`load`) together with `save` (356-361); src/startup.rs:34-46
- Guideline: Error Handling (hidden errors); Correctness (data loss).
- Explanation: When `state.json` fails to parse or read, `load` returns `SavedState::default()` plus a warning. Later `save()` writes the empty state over the file, so a transient problem destroys the breakpoints, watches, trusted folders, plugin grants and run configs. Compare `config::reset_with_backup`, which keeps a `.bak`, and `recent.rs`, which says the next change writes a new file. State holding user work deserves the same care. The warning text also says "using default pane visibility", although the file holds much more than pane visibility (lines 342 and 350).
- Suggested improvement: On a parse failure, rename the bad file to `state.json.bak` before the first save, or refuse to save while the load was unusable. Fix the warning label.

## Minor

### m1. Dead variants: `Event::SetPluginView`, `ActionError::Busy`, `IdeError::Unknown`
- Severity: Minor
- Location: src/event/mod.rs:14 (`SetPluginView`, handled at src/app/mod.rs:486, constructed only in src/tests/app_tests.rs:28 and src/tests/hide_tests.rs:228); src/git/actions.rs:24 (`Busy`, matched at src/app/plugins.rs:271 and 298 but never constructed); src/error.rs:15 (`IdeError::Unknown`, never constructed anywhere; it is `pub`, so no warning).
- Guideline: Readability (dead code); API Design.
- Explanation:
  - `SetPluginView` was superseded by `Event::Plugin(Notice::View)` but is still in the enum and handler, and the tests build it. The tests therefore exercise a path production never takes.
  - `Busy` has match arms that can never fire. The busy refusal now comes from `Refusal::Busy` in `plugins/calls.rs` and `plugins/runs.rs`.
  - `IdeError::Unknown(Box<str>)` is a stringly catch-all that nothing uses.
- Suggested improvement: Delete the variants and their arms. Switch the two tests to `Event::Plugin(Notice::View { .. })`.

### m2. Stale blanket `allow(dead_code)` on `mod plugins` hides real dead code, with an outdated comment
- Severity: Minor
- Location: src/lib.rs:36-39; also src/settings/mod.rs:66-70
- Guideline: Readability; Style (do not silence lints).
- Explanation: The comment says "Nothing loads plugins yet ... most of it is unused", but `app/plugins.rs`, `app/plugin_manager.rs` and others use the module heavily, and the plugin manager and runtime ship. `#[cfg_attr(not(test), allow(dead_code))]` now only suppresses warnings for genuinely unused items. The same applies to `#[allow(dead_code, reason = "the app starts servers in step 3 of the LSP client")]` on `Settings::language_servers`, which is read in app/lsp.rs and app/formatting.rs. `plugins/mod.rs:34-37` carries a similar stale "placeholder until the Git viewer is measured" comment on `CALLBACK_FUEL`.
- Suggested improvement: Remove both `allow`s and fix what clippy then reports. Update or delete the stale comments.

### m3. `Runtime` panics on poisoned mutexes and on thread-spawn failure
- Severity: Minor
- Location: src/plugins/runtime.rs:138, 151, 158-159, 169, 211, 382, 408, 429, 455, 460 (13 sites of `.lock().expect("the lock is not poisoned")`); 329 and 418 (`.expect("the operating system starts a thread")`)
- Guideline: Error Handling (expect/panic); Style (repetition).
- Explanation: These locks are shared between the UI thread and plugin threads. One plugin-thread panic while a lock is held would poison it, and every later UI call (`Routes::calls()`, `runs()`, `send`) would panic, taking the editor down. This is exactly the failure the runtime is designed to isolate. The same `expect` string is repeated 13 times.
- Suggested improvement: Add one `fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T>` that recovers with `PoisonError::into_inner` (the guarded data are simple maps and senders). Return `io::Result` from `Runtime::spawn` and `spawn_owned`, or mark the plugin `Broken`, instead of panicking.

### m4. `Host::deliver` clones the capability set on every message and is long and deeply nested
- Severity: Minor
- Location: src/plugins/runtime.rs:631-716 (`let grants = loaded.grants.clone();` at 658); `answer` at 757-806; `carry_out` at 893-961
- Guideline: Ownership (unnecessary clone/allocation); Style (function size, nested control flow).
- Explanation: `deliver` is about 85 lines with a `loop`, a three-armed `match` and several nested ifs. It also clones a `HashSet<Capability>` for every delivered message, which is the hot path, only to appease the borrow checker (`self.report` needs `&mut self`). `answer` builds a `(id, needs, needs_git)` tuple with a boolean flag. `carry_out` repeats ten near-identical `to_app(...); None` arms.
- Suggested improvement: Keep `grants` as `Arc<HashSet<_>>` in `Loaded`, or compute only the two booleans the code needs before the call. Split `deliver` into "call with restart" and "after a good call". Replace the `needs_git: bool` tuple with a small struct or enum, and have `WriteOp` constructed in one mapping function.

### m5. `FromPlugin::Reply` or `RunCancel` without the capability is silently dropped; the telescoping `load`/`load_on`/`load_with` API
- Severity: Minor
- Location: src/plugins/runtime.rs:849-852, 865-867 (`_ => {}`); 333-376 (`load`, `load_on`, `load_with`)
- Guideline: Error Handling (hidden errors); API Design (poor constructors).
- Explanation: `Call` without `plugin.call` answers `Refused`, but a `Reply` without `plugin.serve`, or a `RunCancel` without `process.run`, falls through to `_ => {}` with no feedback, an inconsistent policy. The three `load*` methods chain with growing positional parameter lists, which is a telescoping-constructor smell.
- Suggested improvement: Document the drop policy at the match, or answer or log consistently. Replace the chain with a `PluginSpec { name, wasm, grants, thread, run }` argument.

### m6. Unrelated output chunks are split in the middle of a UTF-8 character
- Severity: Minor
- Location: src/plugins/exec.rs:243-263 (`read`), 266-277 (`pass_on`)
- Guideline: Correctness (edge cases).
- Explanation: A chunk is flushed when `pending.len() >= MAX_RUN_CHUNK` even mid-line and mid-character. `pass_on` then does `String::from_utf8_lossy`, so a multibyte character at a 64 KiB boundary is turned into two U+FFFD replacement characters. Read errors (`Err`) are also treated as plain EOF (`ended = !matches!(read, Ok(n) if n > 0)`), so an I/O error is not reported. `wait` returns `None` on a `try_wait` error without stopping the child (exec.rs:164-171).
- Suggested improvement: When forced to flush mid-line, cut at the last UTF-8 boundary (`std::str::from_utf8` `valid_up_to`) and carry the remainder to the next chunk. Report or log read errors.

### m7. Duplicated process-supervision code in `git/run.rs`
- Severity: Minor
- Location: src/git/run.rs:290-325 (`run_cancellable`) versus 327-352 (`run_with_timeout`); 168-176 (`run_remote` error mapping) versus 211-217 (`spawn` error mapping)
- Guideline: Readability / Maintainability (duplication); Types (boolean parameter).
- Explanation: The two functions are the same spawn / drain / poll loop, differing only in the cancel check and in killing the whole process group. The error mapping closure is duplicated. `spawn(&self, dir, args, read_only: bool)` takes a boolean that selects two different command setups and timeouts. On timeout, `run_with_timeout` kills only the child (not the group) while `run_cancellable` kills the group. A hook that started a child leaves the drain threads blocked until it exits (documented, but the asymmetry is easy to get wrong).
- Suggested improvement: Make `run_with_timeout` a call to the cancellable version with a never-set `AtomicBool`. Replace `read_only: bool` with `enum Access { Read, Write }`. Extract the shared error mapping.

### m8. `index_lines` reads the whole blob before enforcing `MAX_BASE_BYTES`
- Severity: Minor
- Location: src/git/run.rs:98-121
- Guideline: Performance / Ownership (unbounded allocation).
- Explanation: `run(root, ["show", ":0:path"])` collects all stdout (`drain` uses `read_to_end`) into memory. The 8 MiB check happens afterwards via `.filter(|text| text.len() <= MAX_BASE_BYTES)`. A large staged blob (a data file, a video) is read fully into memory, repeatedly, each time a tab is opened or the file's git state changes. The doc says "too big" returns `None`, but only after the allocation.
- Suggested improvement: Cap the pipe reader (`take(MAX_BASE_BYTES + 1)`), or check size first with `git cat-file -s`. `drain` could take a limit parameter.

### m9. Stringly-typed git command names and locale-dependent output matching
- Severity: Minor
- Location: src/git/run.rs:130-142 (`read`: `match (name, outcome.code)` with `"rev-parse"`); src/git/remote.rs:120-134 (`needs_secret` matches English phrases, including `"no such device or address"`); src/git/run.rs:112 (exit code 128 means "not in index")
- Guideline: Types (strong types over primitives and strings); Correctness.
- Explanation: Error classification relies on matching git's English messages. The environment does not force `LC_ALL=C`. Git is localised, so on, for example, a Dutch locale the authentication hint (`NeedsAuthentication`) is never produced and the user gets a generic error. `read` identifies the command by comparing a string literal with the first argument.
- Suggested improvement: Set `LC_ALL=C` (and `LANGUAGE=C`) on git invocations whose output is parsed, or classify by exit code plus stderr under `LC_ALL=C`. Pass a `GitCommand` enum instead of `&str` names.

### m10. `slashed` duplicated with different semantics; one corrupts names on Unix
- Severity: Minor
- Location: src/git/run.rs:224-230 versus src/plugins/git_view.rs:105-107
- Guideline: Correctness; Maintainability (duplication).
- Explanation: `git_view::slashed` does `path.to_string_lossy().replace('\\', "/")`. On Unix a backslash is a legal file-name character, so a file `a\b.txt` is reported to plugins as `a/b.txt`, and a later `git_stage` for that path would target the wrong file. `git::run::slashed` joins components and is correct on both platforms.
- Suggested improvement: Reuse one function (expose `git::run::slashed`, or put it in `files::paths`).

### m11. `Action::title`, `palette_actions`, `needs_editor` and `focuses_editor` are parallel hand-maintained tables
- Severity: Minor
- Location: src/event/action.rs:332-654 (`focuses_editor`, `needs_editor` with about 90 variants in a negated `matches!`, `title`, `palette_actions`)
- Guideline: Maintainability (readability, repeated knowledge).
- Explanation: A new action has to be added to up to four lists. `needs_editor` is a negated list with an implicit default of `true`, so a forgotten action silently becomes "needs an open file" and does nothing when all tabs are closed. `title()` and `palette_actions()` must agree (an action with a title but no palette entry is invisible), and nothing in the code enforces that.
- Suggested improvement: Drive the palette from one static table (variant plus title plus flags), or at least add a test asserting that every action with a `title()` appears in `palette_actions()`. Prefer positive `matches!` lists with explicit defaults.

### m12. Boolean parameters and flags
- Severity: Minor
- Location: src/settings/update.rs:192, 235, 243 (`unsaved_tab: bool`; startup passes a bare `false` at src/startup.rs:38); src/terminal/mod.rs:100 (`set_mouse_capture(enabled: bool)`); src/plugins/calls.rs:90-96 (`reply(..., last: bool)`); src/plugins/runtime.rs:226 (`set_workspace(root, trusted: bool)`)
- Guideline: Functions (avoid boolean parameters); Types (enums over booleans).
- Explanation: Call sites like `update_file(&settings, &mut saved, false)` do not say what `false` means.
- Suggested improvement: `enum TabState { Clean, Unsaved }`, `enum Trust { Trusted, Untrusted }`, `enum Mouse { On, Off }`.

### m13. `startup::load_user_config` is a 90-line multi-purpose function
- Severity: Minor
- Location: src/startup.rs:29-115
- Guideline: Functions (one responsibility, size).
- Explanation: It updates the user's settings file on disk, saves state, loads six config files, reconciles saved versus layout visibility, and mutates `AppState`, all in one body with a `report` closure capturing `state`. The disk write to the user's `settings.json` at startup is also easy to miss in a function named "load".
- Suggested improvement: Split into `update_settings_file`, `load_files` (returns a struct plus a `Vec<ConfigWarning>`) and `apply_visibility(state, saved, layout)`.

### m14. Atomic-write helper duplicated three times; one copy does not create the folder
- Severity: Minor
- Location: src/persist/saved_state.rs:365-374 (`write_atomically`); src/plugins/trust.rs:111-120 (`Pins::save`); src/plugins/index.rs:265-270 (`store_cache`); src/persist/recent.rs:127 reaches into `saved_state` for the helper
- Guideline: Maintainability (duplication); API Design (misplaced helper).
- Explanation: Three near-identical write-temp-then-rename routines exist. `store_cache` does not call `create_dir_all`. All three use a fixed temp name, so two editor instances race. None fsyncs before rename. `write_atomically` lives in `saved_state`, although it is a general persist utility used by `recent` and not by anything about saved state.
- Suggested improvement: Move one `write_atomically` to `persist/mod.rs` (or `config/`), use a unique temp name, and call it from all three.

### m15. `empty_text` hides a serialization failure by writing an empty file
- Severity: Minor
- Location: src/persist/saved_state.rs:322-325
- Guideline: Error Handling (hidden errors).
- Explanation: `serde_json::to_string_pretty(...).unwrap_or_default()` would write `"\n"` as the new `state.json` on failure, which would then fail to parse on the next start (see M3).
- Suggested improvement: Return `io::Result<String>` or `Result<String, serde_json::Error>`.

### m16. `ConfigFile::create_with` ignores `self`, and leaves a truncated file on a failed write
- Severity: Minor
- Location: src/config/files.rs:63-83
- Guideline: API Design (poor encapsulation); Correctness.
- Explanation: `create_with(self, path, content)` never uses `self`, so it is an associated function pretending to be a method. It creates the file with `create_new`, then `write_all`. If the write fails (disk full), the empty or partial file stays, and every later call returns `Ok(false)` ("already exists"), so the user is stuck with a broken file.
- Suggested improvement: Make it a free function, and write to a temp file and rename (or remove the file on a failed write).

### m17. `install()` pins the module hash before the install has succeeded
- Severity: Minor
- Location: src/plugins/install.rs:317-322 (`Pins::trust`), then `build_and_swap` at 331-369
- Guideline: Correctness (ordering of side effects).
- Explanation: Trust-on-first-use pins `(id, version, hash)` in `.trust.json` before staging, validating the manifest and swapping. If `build_and_swap` fails afterwards (`NotWhatTheIndexSaid`, a bad manifest, an IO error), the pin stays even though that version was never installed. A later legitimate re-publish of the same version is then refused as "changed" until the user hand-edits `.trust.json`.
- Suggested improvement: Pin only after `build_and_swap` succeeds (check first, record at the end).

### m18. Stringly errors in plugin/install and update layers
- Severity: Minor
- Location: src/plugins/install.rs:77-109 (`InstallError::Network(String)`, `Io(String)`, `From<io::Error>` stringifies), 135-155 (`JobEvent` with `Result<_, String>`); src/settings/update.rs:131-150 (`Refusal::Io(String)`); src/plugins/manifest.rs (`Unreadable(String)`, `Invalid(String)`); src/plugins/trust.rs:25-34 (`TrustError::File(String)`); src/plugins/instance.rs:10-18 (`Failure(String)`)
- Guideline: Error Handling (generic String errors; concrete error types).
- Explanation: Errors are converted to `String` at the first boundary, which loses `ErrorKind` and the source chain. `JobEvent` carries `Result<_, String>` across the event channel, so the app cannot match on, for example, `Cancelled`. This is partly for `Clone + PartialEq`, but `io::Error` can be kept as `Arc<io::Error>` or as `ErrorKind` plus message.
- Suggested improvement: Keep an `io::ErrorKind` (or `#[source] Arc<io::Error>`) in the variants, and send the typed `InstallError` (or a `Clone` summary of it) in `JobEvent`.

### m19. Hand-written `Display` impls next to `thiserror` everywhere else
- Severity: Minor
- Location: src/settings/update.rs:152-165 (`Refusal`), src/plugins/trust.rs:36-56 (`TrustError` with a manual `impl Error`)
- Guideline: Style / consistency with the project's error approach.
- Explanation: The rest of the crate uses `thiserror`; these two re-implement it by hand.
- Suggested improvement: Derive `Error`.

### m20. `Marker` can be unreadable and look like a user-made plugin
- Severity: Minor
- Location: src/plugins/install.rs:182-184 (`marker`: `.ok()?` on both read and parse)
- Guideline: Error Handling (hidden errors).
- Explanation: A corrupt `installed.json` makes `marker()` return `None`, so an installer-made folder is treated as "a plugin of your own": `install` then returns `Local` ("remove it by hand"), `remove` returns `NotManaged`, and `verify_module` skips the hash check entirely (src/plugins/trust.rs:131-134). The integrity check is therefore bypassed by corrupting or deleting one small file next to the module.
- Suggested improvement: Distinguish "no marker file" from "marker unreadable". Treat the latter as an error, and do not skip verification for it.

### m21. `Policy::allows` precedence and `https_only` handling is easy to misread
- Severity: Minor
- Location: src/plugins/install.rs:68-83
- Guideline: Readability.
- Explanation: `if self.https_only && scheme != "https" || !matches!(scheme, "http" | "https")` relies on `&&` binding tighter than `||`. The port-stripping branch only runs for non-https policies, with a comment that does not say why (the test-only local policy).
- Suggested improvement: Add parentheses (or two early returns) and a short comment.

### m22. File layout violations (globals first, then `pub`, then private)
- Severity: Minor
- Location:
  - src/terminal/mod.rs:38-41 (four `static`s after `pub` types) and 111 (`const COLOR_QUERY_TIMEOUT` after pub fns)
  - src/settings/update.rs:44 (`const BASELINE`, `INTRODUCED` after `pub(crate) struct Version`), 109 (test `example` module mid-file)
  - src/plugins/exec.rs:50-52 (private consts `RUNNING/CANCELLED/LOST` after `pub(super)` items)
  - src/plugins/mod.rs:20-40 (module declarations, then consts)
  - src/lib.rs:12-39 (`use` aliases between `mod` lines)
  - src/git/run.rs:196-210 (pub methods mixed with private `read` between `pub(crate)` ones)
  - src/config/mod.rs:27 (`const CONFIG_DIR` after `pub(crate) use`)
- Guideline: Style, file layout: globals first, then `pub` items most-to-least relevant, then private.
- Explanation: Constants and statics should come first so a reader sees them before any implementation.
- Suggested improvement: Reorder. Group `const` / `static` at the top of each file.

### m23. `RUNNING/CANCELLED/LOST: u8` constants instead of an enum, with a test-only accessor returning a magic number
- Severity: Minor
- Location: src/plugins/exec.rs:50-52, 58-73 (`Control::stop: AtomicU8`, `stopped_for_test() -> u8` documented as "0 not, 1 cancelled, 2 lost")
- Guideline: Types (enums over primitives).
- Explanation: A tri-state is stored as three `u8` constants and compared as numbers. Tests inspect the raw number.
- Suggested improvement: `#[repr(u8)] enum Stop { Running, Cancelled, Lost }` with `TryFrom<u8>`, or a `Mutex<Stop>`. Have the test accessor return the enum.

### m24. Three different types named `Host` inside `plugins`
- Severity: Minor
- Location: src/plugins/instance.rs:39 (`Host { limits, sent }`), src/plugins/runtime.rs:511 (`Host`), src/plugins/manifest.rs:93 (`Host<'a>`)
- Guideline: Readability (poor naming).
- Explanation: Three unrelated types share a name, and `runtime.rs` imports `instance::*` types next to its own `Host`. Search and review are harder than they need to be.
- Suggested improvement: Rename them: `HostState` (instance), `PluginThread` (runtime), `EditorInfo` (manifest).

### m25. Saved state stores paths and versions as `String`
- Severity: Minor
- Location: src/persist/saved_state.rs:125-141 (`trusted_folders: BTreeSet<String>`, `active_runs`, `breakpoints`, `watches` keyed by `String` "whole path"); `settings_version: Option<String>`; src/plugins/index.rs:38-58 (`Entry.version`, `sha256`, `abi` as `String` validated later by `check()`)
- Guideline: Types (strong types over primitives).
- Explanation: Folders are `String` keys made from `canonicalize` plus `to_string_lossy`, which cannot round-trip non-UTF-8 paths, and `Version` already exists in `settings/update.rs`. The index entry is validated after deserialisation instead of parsed into types.
- Suggested improvement: Use `PathBuf` keys (serde supports them), use the existing `Version` type, and use `serde(try_from)` for entries.

### m26. Minor correctness and wording nits in `keymap` / `config`
- Severity: Minor
- Location:
  - src/keymap/mod.rs:118, 126 (panic text says "tyring"; two near-identical `panic!` blocks)
  - src/config/warning.rs:28-30 (`NoSuchTheme` hard-codes "settings.json")
  - src/config/theme.rs:~143 versus 225 (`installed_theme_path` and `theme_file_stem` use different name validation rules; `.hidden` resolves but cannot be created)
- Guideline: Error Handling (`panic!`, but only for an embedded-default invariant; acceptable if clear); Readability.
- Explanation: These are a typo in a user-visible panic message and inconsistent validation. The `defaults` panic is an acceptable invariant violation, but it should be one helper and a unit test (one probably exists).
- Suggested improvement: Fix the typo, merge the two blocks, and share a single `is_plain_file_name` helper.

### m27. `git::lines::line_hashes` swallows errors and allocates a `String` per line on every edit
- Severity: Minor
- Location: src/git/lines.rs:104-111
- Guideline: Error Handling (hidden errors); Performance.
- Explanation: `Err(_) => hash_line("")` hides a failure as an empty line. Each buffer version rehashes the whole document by allocating a `String` per line (`line_content`).
- Suggested improvement: Do not substitute silently (an out-of-range line is an invariant error), and hash a line slice without allocating if the buffer API allows. Per the guidelines, only do the latter after measuring.

### m28. Production `expect` in `git/diff.rs` is avoidable
- Severity: Minor
- Location: src/git/diff.rs:185
- Guideline: Error Handling.
- Explanation: `blocks.last_mut().expect("a block was just pushed")` can be written as `match blocks.last_mut()` with an `if !joins { push }` that returns the reference, with no panic path. Edits are also a `(usize, usize, bool)` tuple, where `bool` means "removes".
- Suggested improvement: Use `enum Edit { Remove, Add }` and restructure so the block reference comes from `push`/`last_mut` without `expect`.

## Test suites

### T1. Process-liveness assertions are environment-dependent and can fail on a zombie
- Severity: Minor
- Location: src/git/tests.rs:1811-1817 (`kill(child, 0)`), src/plugins/exec/tests.rs:191, src/plugins/tests/runs.rs:70 (same `kill(pid, 0) == 0` helper)
- Guideline: Testing (flakiness; not brittle).
- Explanation: `kill(pid, 0)` returns success for a zombie. If the orphaned grandchild is not reaped (a container whose PID 1 does not reap, or a CI runner without `--init`), the test fails although the code is right. The `sleep(200ms)` before the check is a guess. The three test copies of the helper also duplicate `libc` unsafe.
- Suggested improvement: Read `/proc/<pid>/stat` state (or use `waitpid`/pidfd) and treat `Z` as dead, and poll with a deadline instead of a fixed sleep. Share one helper.

### T2. Real-clock sleeps drive several app tests
- Severity: Minor
- Location: src/tests/hover_tests.rs:19, 65, 145, 158-160, 255, 290, 440, 691, 814 (`LONG_ENOUGH = 450ms`, 250 ms plus 250 ms "half a second in two goes"); src/tests/run_tests.rs:467 (`sleep(1100ms)` to get a different mtime second); src/tests/plugin_menu_tests.rs:223, 259 (`sleep(200ms)` then assert something did not happen)
- Guideline: Testing (not brittle; flakiness).
- Explanation: These tests assert behaviour that depends on wall-clock thresholds (the hover delay) and sleep to "give something time not to happen". Under load the second 250 ms sleep can overshoot the threshold and change the outcome, and the suite pays seconds of dead time. A negative assertion after a sleep proves little.
- Suggested improvement: Inject the clock (as `Schedule` and `ClickTracker` already do with `Instant`), and for mtime use `File::set_modified`.

### T3. The `wait_until` polling loop is copied into many test files
- Severity: Minor
- Location: src/tests/plugin_menu_tests.rs:78, plugin_files_tests.rs:106, plugin_column_tests.rs:79 and 100, plugin_place_tests.rs:109, plugin_tests.rs:65, format_tests.rs:108 and 112, run_tests.rs:205, git_tests.rs:76 and 84, root_save_tests.rs:115, rename_symbol_tests.rs:427 (each its own `sleep(5ms)` loop with its own timeout)
- Guideline: Testing ("Keep test helpers small and focused"); Maintainability (duplication).
- Explanation: Each rig re-implements poll-with-timeout, with varying deadlines and failure messages. Likewise the `git(dir, args)` helper is copied in src/git/tests.rs:298, src/tests/git_tests.rs:239, plugin_git_tests.rs:44 and real_plugin_tests.rs:64.
- Suggested improvement: One `tests/support` module with `wait_until(what, timeout, cond)` and a `git()` helper.

### T4. `git/tests.rs` is a 1842-line grab bag and depends on the host `git` configuration
- Severity: Minor
- Location: src/git/tests.rs (98 tests, 148 `unwrap()`); real-git helpers at 296-312
- Guideline: Testing (public behaviour, no external dependence); Style (file size).
- Explanation: One file holds parser, schedule, diff, marks, lines, actions and real-git tests, so it does not mirror module boundaries. The real-git tests need `git` on PATH and `git init -b main` (git 2.28 or later), and they inherit the developer's global git config (`init.templateDir`, `core.hooksPath`, `safe.directory`, `commit.gpgsign` is overridden but others are not). A fake-git stand-in loop at ~line 430 retries `ExecutableFileBusy` with no upper bound, so a persistent ETXTBSY would hang.
- Suggested improvement: Split by module (`parse_tests`, `diff_tests`, `schedule_tests`, `real_git_tests`). Set `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_SYSTEM=/dev/null` and a template dir in the helper. Bound the retry loop.

### T5. Implementation-coupled tests
- Severity: Suggestion
- Location: src/tests/root_save_tests.rs:236-242 (reads freed secret memory through `from_raw_parts` on `raw_for_test().as_ptr()` after `clear()`; the comment says the vector keeps its capacity, which is an implementation detail and only sound if no reallocation happened); src/plugins/exec.rs:68-72 (`stopped_for_test`); `Git::with_program` (src/git/run.rs:42-50) and `Control` test accessors; src/plugins/tests.rs and src/tests/plugin_manage_tests.rs assert exact raw JSON text of plugin views and refusals (for example `r#"{"type":"refused","request":5,"reason":{"type":"untrusted"}}"#`).
- Guideline: Testing (not coupled to implementation details; avoid unnecessary unsafe).
- Explanation: Exact-JSON assertions fail on harmless field reordering or the addition of optional fields. The `from_raw_parts` read is UB-adjacent (reading spare capacity) and tests the allocator layout rather than behaviour.
- Suggested improvement: Parse the message and compare structure (or compare `ToPlugin` values). For the secret test, check through a safe accessor on the type.

### T6. `unwrap()` density in tests
- Severity: Suggestion
- Location: about 1,620 `unwrap()` calls across src/tests, src/plugins, src/settings, src/keymap, src/config, src/persist test files; 148 in src/git/tests.rs
- Guideline: Testing ("tests avoid `unwrap()` where an assertion is feasible"; `unwrap()` is acceptable when failure means a test bug).
- Explanation: Most setup `unwrap()`s are fine. The ones on the code under test (for example `parse_status(&answer).unwrap()`, `Git::default().status(..).unwrap()`) hide the failure context compared with `assert!(matches!(..))` or `assert_eq!(result, Ok(..))`, though `Result::unwrap` does print the error. Not a defect, but it is the largest deviation from the written testing guideline.
- Suggested improvement: Have tests return `Result<(), Box<dyn Error>>` and use `?`, or assert on the result directly where the value matters.

### T7. Tests tied to the repository checkout and the real working directory
- Severity: Suggestion
- Location: src/plugins/exec/tests.rs:43, 106, 143 (`cwd: std::env::current_dir()`); src/tests/plugin_manage_tests.rs:755, 783 (trust-folder tests use `current_dir()` as the project); src/tests/real_markdown_tests.rs:119 (`CARGO_MANIFEST_DIR/README.md`, though that test is `#[ignore]`)
- Guideline: Testing (filesystem tests use temp dirs).
- Explanation: None of these writes into the repo (I found no violations of the "do not write in the repo" rule; tests use `tempfile`). The trust tests do record the repo folder as trusted in the in-memory saved state, tying them to cwd, which is the package root only under `cargo test`. `src/tests/open_tests.rs:58` depends on `dirs::home_dir()`.
- Suggested improvement: Use a `tempdir` as the project folder.

## Suggestions

### S1. `Index::parse` parses the JSON twice
- Severity: Suggestion
- Location: src/plugins/index.rs:197-223
- Guideline: Performance (unnecessary work), Readability.
- Explanation: It deserializes `Head { format }` and then `Body { plugins }` from the same text. One struct with both fields (and `#[serde(default)]` on `plugins`) would do, with the version check after.
- Suggested improvement: Parse once; check `format` before validating entries.

### S2. `Compiled::new` builds an `Engine` per plugin
- Severity: Suggestion
- Location: src/plugins/instance.rs:60-70
- Guideline: Ownership / Performance (only after measuring).
- Explanation: Each plugin gets its own wasmi `Engine`. An `Engine` is cheap to clone and designed to be shared, but whether this matters needs measuring. The store also sets no table or instance-count limits (`StoreLimitsBuilder` sets `memory_size` and `instances` only).
- Suggested improvement: Share one `Engine` per thread or runtime. Add `table_elements` and `tables` limits if the plugin ABI does not need them.

### S3. `terminal::drain::Tty::read` treats `poll` errors as a timeout
- Severity: Suggestion
- Location: src/terminal/drain.rs:101-105
- Guideline: Error Handling (hidden errors).
- Explanation: `if ready <= 0 { return None; }` treats `EINTR` (`-1`) like "nothing arrived", which ends the drain early and can leave mouse reports in the tty. The unsafe block itself is fine: its SAFETY comment is correct and the fd stays open for the call.
- Suggested improvement: Retry on `EINTR` (`io::Error::last_os_error().kind() == Interrupted`).

### S4. `ThemeChoice::installed` field and constructor share a name; `Status`, `Entry`, `Branch`, `PluginRecord` expose `pub` mutable fields
- Severity: Suggestion
- Location: src/config/theme.rs:25-35; src/git/mod.rs:61-100; src/persist/saved_state.rs:92-110
- Guideline: API Design (avoid public mutable fields).
- Explanation: These are `pub(crate)` data carriers, so the impact is small. Nothing enforces invariants such as `Branch::ahead/behind` or `PluginRecord::capabilities` being sorted.
- Suggested improvement: Keep as is unless an invariant appears. If so, add constructors and private fields.

### S5. `git::actions::output_of` and `Action::palette_actions` build `Vec`s that could be iterators or constants
- Severity: Suggestion
- Location: src/git/actions.rs:128-165; src/event/action.rs:525
- Guideline: Ownership / Performance (unnecessary allocation), only if measured.
- Explanation: `output_of` collects `lines` and then re-collects a `Vec<&str>` to join. `palette_actions()` allocates a 120-entry `Vec` each time the palette opens (cold path). Neither is worth changing without evidence.
- Suggested improvement: Leave, or return `&'static [Action]` where the variants carry no data.

## Items checked and found fine

- Every `unsafe` block has a SAFETY comment: src/plugins/exec.rs:210 (invariant is wrong on one path, see M2), src/git/run.rs:263 (`setsid` in `pre_exec` is async-signal-safe) and 280 (`kill(-pid)` before `wait`, so valid), src/terminal/drain.rs:102 (valid `pollfd`, count 1). Test-side unsafe: git/tests.rs:1815, plugins/exec/tests.rs:191, plugins/tests/runs.rs:70 (`kill(pid, 0)`), root_save_tests.rs:240 (see T5).
- `git/diff.rs` is bounded (`MAX_EDITS = 500`) in both time and memory, with a sensible fallback to "whole stretch changed".
- `plugins/install.rs` validates entry ids (`is_id`) at index parse time before they are used in paths, restricts download hosts (`Policy`), caps body size, checks the SHA-256 before anything reaches the plugins folder, and swaps by rename with rollback.
- `plugins/runs.rs::folder_in` canonicalises and checks containment, so a plugin cannot run a command outside the project.
- `keymap::apply_toml` does leave the keymap unchanged on invalid input, as documented.
- `src/cli.rs` handles `--`, non-UTF-8 paths and flag ordering with focused tests.
- No test writes into the repository; temp dirs are used. The `real_*` suites that need external checkouts or `sudo` are `#[ignore]`d with clear reasons.
