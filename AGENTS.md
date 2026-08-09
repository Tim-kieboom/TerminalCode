# AGENTS.md

## Project

**TerminalCode** — a lightweight, keyboard-driven CLI IDE built with Rust + Ratatui.

## Coding Guidelines

This repository follows the Rust coding standards defined in the project root.

- **`Guideline PROMPT CODE.md`** — Read and follow this file whenever generating, modifying, or refactoring Rust code. These rules are mandatory unless explicitly instructed otherwise.

- **`Guideline PROMPT REVIEW.md`** — Use this file whenever reviewing, auditing, or providing feedback on existing Rust code. Evaluate all code against these guidelines and report any violations.

When writing code, prioritize:

1. Correctness
2. Readability
3. Maintainability
4. Performance (only after measuring)

If a requested implementation conflicts with the coding guidelines, explain the conflict and produce the closest guideline-compliant solution.

## Commands

- Build: `cargo build`
- Fast compile check: `cargo check`
- Lint: `cargo clippy` (use `--all-targets` to also lint test code)
- Format: `cargo fmt` (verify with `cargo fmt --check`)
- Run: `cargo run`
- Test: `cargo test`

## Notes

- `cargo build` fails to link while `terminal_code.exe` is running. Stop the application before rebuilding.
- Unit tests live in sibling `tests.rs` files (`#[cfg(test)] mod tests;`), per the coding guidelines.
- `lib/vecmap` is a separate crate with its own tests; run them with `cargo test` inside `lib/vecmap`.