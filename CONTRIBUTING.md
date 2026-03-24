# Contributing

## Branching And Git Policy

Use `dev` as the integration branch. Do not branch from `main` for normal feature work.

Recommended workflow:

1. Create a focused branch from `dev`.
2. Use a branch name that reflects the work, for example:
   - `phase/1-buffer`
   - `phase/4-preview`
   - `feature/sidebar-file-ops`
3. Keep commits small and task-shaped.
4. Write commit messages in lowercase, present tense:
   - `feat: add text buffer`
   - `fix: handle backspace at line start`
   - `chore: bump version to 0.1.7`
5. Merge completed branches back into `dev` with a regular merge commit.
6. Reserve `main` for tagged releases only.

Do not:

- force-push `dev`
- force-push `main`
- rewrite shared branch history unless the team explicitly agrees to it

## Project Layout

Keep the application code inside the Rust project root under `ImplicitDraft-src/`. The repository root is for shared docs and planning notes.

Use these directories consistently:

- `ImplicitDraft-src/`
  - Cargo project root
  - `Cargo.toml`
  - `src/`
  - tests
- `ImplicitDraft-output/`
  - compiled artifacts via `CARGO_TARGET_DIR`
- `ImplicitDraft-tmp/`
  - scratch files only
  - never commit

## Rust Organization

Organize code by feature instead of deep layer nesting.

Preferred module shape:

- `buffer`
- `picker`
- `render`
- `config`
- `sidebar`
- `preview`

Prefer explicit state types such as `App`, `Buffer`, `Theme`, and `VimState`.

## Style And Naming

Follow standard Rust conventions:

- 4-space indentation
- `snake_case` for files, modules, and functions
- `PascalCase` for structs, enums, and traits
- `SCREAMING_SNAKE_CASE` for constants

Keep modules focused. Split files when a module starts carrying unrelated responsibilities.

## Build And Test

Run commands from `ImplicitDraft-src/` unless noted otherwise.

Core commands:

- `cargo run -- path/to/doc.md`
- `cargo run`
- `cargo build`
- `CARGO_TARGET_DIR=../ImplicitDraft-output cargo build --release`
- `cargo test`
- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`

Treat `cargo fmt`, `cargo test`, and `cargo clippy` as the default verification bar before merging.

## Testing Guidelines

Use Rust's built-in test framework.

- Put unit tests next to the code they cover.
- Put integration tests under `ImplicitDraft-src/tests/`.
- Name tests after observable behavior:
  - `loads_markdown_file`
  - `warns_on_quit_with_unsaved_changes`
  - `opens_sidebar_with_focus`

Prefer behavior-oriented tests over implementation-specific assertions.

## Pull Requests

Keep pull requests narrow and easy to review.

A good PR should:

- solve one coherent problem
- describe user-visible behavior changes
- mention any important tradeoffs or known gaps
- include the verification commands that were run

If the work is part of a phase plan, reference that phase directly in the PR description.

## Reusable Team Policy

If you want to reuse this structure in another repo, the portable version is:

1. `main` is release-only.
2. `dev` is the integration branch.
3. Feature branches come from `dev`.
4. Shared branches do not get force-pushed.
5. Code is organized by feature boundaries.
6. `fmt`, tests, and lints must pass before merge.
