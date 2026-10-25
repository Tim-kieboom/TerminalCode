# Rust Coding Agent

You are writing Rust code for a company that prioritizes:

1. Correctness
2. Readability
3. Maintainability
4. Performance (only after measuring)

Always follow the project's Rust Coding Guidelines.

## Required behavior

Before writing code:

- Think about ownership.
- Think about API design.
- Think about visibility.
- Prefer simple solutions.

When multiple valid implementations exist, choose the one that is easiest to understand.

---

## Always follow these rules

### Visibility

Prefer

private
→ pub(super)
→ pub(crate)
→ pub

Never use `pub` unless required.

---

### Ownership

- Borrow instead of clone.
- Avoid unnecessary allocations.
- Reserve capacity when size is known.
- Prefer immutable references.
- Avoid `Rc`, `Arc`, and interior mutability unless required.

Never clone just to satisfy the borrow checker.

---

### Functions

Functions should

- have one responsibility
- use early returns
- avoid deep nesting
- avoid boolean parameters
- avoid excessive parameters (>6)

Prefer

```rust
let Some(value) = option else {
    return Err(...);
};
```

instead of nested `if`.

---

### Types

Prefer

- enums over booleans
- strong types over primitives
- structs with named fields
- composition over inheritance
- generics / impl Trait over dyn Trait

Avoid public mutable fields.

---

### Error Handling

Use

- Result
- Option
- ?
- let ... else
- match

Avoid

- unwrap()
- expect()
- panic!()

unless violating an internal invariant.

Use concrete error types.

Only use anyhow::Result for application entry points or orchestration.

---

### Performance

Never optimize without evidence.

Prefer

- good algorithms
- fewer allocations
- fewer clones
- cache-friendly data
- stack allocation

before micro-optimizations.

---

### Unsafe

Unsafe code is only acceptable when

- safe Rust cannot express it
- measurable benefit exists
- invariants are documented

Every unsafe block requires a SAFETY comment.

---

### Style

Prefer explicit code over clever code.

Avoid complicated iterator chains when explicit control flow is easier to understand.

Readable code always wins.

#### File layout

Order the top of every file like this:

1. Global items (`const`, `static`) first.
2. `pub` items next, ideally sorted from most relevant to least relevant.
3. Private items and helpers after that.

A reader should see a file's constants and public surface before any implementation detail.

---

### Standard APIs

Prefer Rust conventions:

- new
- Default
- From
- TryFrom
- AsRef
- Into
- Iterator
- IntoIterator

Do not invent custom APIs.

---

### Testing

Write unit tests in separate test files, not inline `mod tests` blocks.

- Keep each module's tests in a `tests` submodule of that module, declared with `#[cfg(test)] mod tests;`: the tests of a directory module `src/foo/` are in `src/foo/tests/`, one file for each source file (`src/foo/bar.rs` → `src/foo/tests/bar.rs`, listed in `src/foo/tests/mod.rs`). A small module may have a single `tests.rs` instead. Do not put tests in `{name}_tests.rs` files next to the source, or in a `tests` folder of an unrelated module.
- A test file imports what it tests through the module's path (`use crate::foo::bar::{...}`), and takes a helper from the module that owns it (for example `path_in` of `saved_state`, not of `recent`).
- Test public behavior and edge cases, not implementation details.
- Keep test helpers small and focused.
- Use temporary directories (for example `tempfile::tempdir()`) for filesystem tests instead of writing into the repo.
-  `unwrap()` is acceptable when the failure cannot happen without a test bug.
- Name tests after the behavior under test.

---

### Output

Generate production-quality Rust.

Do not explain obvious Rust concepts.

If a requested implementation violates the guidelines, explain why and produce a guideline-compliant alternative.
