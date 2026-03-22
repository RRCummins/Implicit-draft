# Repository Guidelines

## Project Structure & Module Organization
This project is planned as a Rust terminal markdown editor built with `ratatui` and `crossterm`. Treat `ImplicitDraft-src/` as the Cargo project root and keep all application code there. Reserve the repository root for shared docs such as this guide and planning notes.

Use these directories consistently:
- `ImplicitDraft-src/` for Rust source, tests, and `Cargo.toml`
- `ImplicitDraft-output/` for compiled artifacts via `CARGO_TARGET_DIR`
- `ImplicitDraft-tmp/` for scratch files only; never commit it

Organize Rust code by feature (`buffer`, `picker`, `render`, `config`) rather than by layer-heavy nesting.

## Build, Test, and Development Commands
Run commands from `ImplicitDraft-src/` unless noted otherwise.

- `cargo run -- path/to/doc.md` launches the editor with a file
- `cargo run` launches the no-arg file picker flow
- `cargo build` builds a debug binary
- `CARGO_TARGET_DIR=../ImplicitDraft-output cargo build --release` writes release artifacts outside the source tree
- `cargo test` runs unit and integration tests
- `cargo fmt` formats the codebase
- `cargo clippy --all-targets --all-features -D warnings` enforces lint cleanliness

## Coding Style & Naming Conventions
Follow standard Rust style: 4-space indentation, `snake_case` for files, modules, and functions, `PascalCase` for structs/enums/traits, and `SCREAMING_SNAKE_CASE` for constants. Keep modules small and focused. Prefer explicit state types such as `App`, `Buffer`, `Theme`, and `VimState`.

## Testing Guidelines
Use Rust’s built-in test framework. Place unit tests next to the code they cover and add integration tests under `ImplicitDraft-src/tests/` for editor flows and parser behavior. Name tests after observable behavior, for example `loads_markdown_file` or `warns_on_quit_with_unsaved_changes`.

## Commit & Pull Request Guidelines
Branch from `dev`, not `main`, using phase branches such as `phase/1-buffer` or `phase/4-preview`. Keep commit messages lowercase and present tense: `feat: add text buffer`, `fix: handle backspace at line start`.

Do not force-push `dev` or `main`. Merge completed phase branches into `dev` with a regular merge commit, and reserve `main` for tagged releases. If an AI agent prepares changes, it should not create commits without explicit approval.
