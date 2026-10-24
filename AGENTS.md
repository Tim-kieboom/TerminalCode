# AGENTS.md

## Project

**TerminalCode** — a lightweight, keyboard-driven CLI IDE built with Rust + Ratatui.

## Coding Guidelines

This repository follows the Rust coding standards defined in the project root.

- **`docs/Guideline PROMPT CODE.md`** — Read and follow this file whenever generating, modifying, or refactoring Rust code. These rules are mandatory unless explicitly instructed otherwise.

- **`docs/Guideline PROMPT REVIEW.md`** — Use this file whenever reviewing, auditing, or providing feedback on existing Rust code. Evaluate all code against these guidelines and report any violations.

When writing code, prioritize:

1. Correctness
2. Readability
3. Maintainability
4. Performance (only after measuring)

If a requested implementation conflicts with the coding guidelines, explain the conflict and produce the closest guideline-compliant solution.

## Commands

- Build: `cargo build`
- Fast compile check: `cargo check --workspace`
- Lint: `cargo clippy --workspace` (use `--all-targets` to also lint test code)
- Format: `cargo fmt --all` (verify with `cargo fmt --all --check`)
- Run: `cargo run`
- Test: `cargo test --workspace` (plain `cargo test` only tests the `trcode` package, not the crates in `crates/`)

## Notes

- **`docs/todo.md`** should be the todo list
- **`docs/design.md`** should be the design document
- `cargo build` fails to link while `trcode.exe` is running. Stop the application before rebuilding.
- Unit tests live in sibling `tests.rs` files (`#[cfg(test)] mod tests;`), per the coding guidelines.
- The repository is a Cargo workspace: the editor is the root package, and the leaf crates it is built from are in `crates/` (`trcode-ids`, `trcode-buffer`, `trcode-files`, `trcode-pty`, `trcode-lsp`, `trcode-syntax`, `trcode-clipboard`, `trcode-jumplist`, `trcode-launch`, `trcode-ui`, `trcode-plugin-api`). A crate depends only on crates below it; see "Workspace" in `docs/design.md`.
