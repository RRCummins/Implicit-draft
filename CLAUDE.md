# CLAUDE.md

This repository vendors a small shared Rust skill bundle for both Codex and Claude.

## Shared Skills

- Project-local skill copies live under `.agent-skills/`.
- Claude plugin metadata lives at `.claude-plugin/marketplace.json`.
- The selected skills are `rust-core`, `lint-hunter`, `general-debug`, and `general-security`.

## How To Use Them

- Use `Rust Core` for feature work, refactors, and general Rust implementation.
- Use `Lint Hunter` when `cargo check`, `cargo test`, or `clippy` surfaces compiler diagnostics.
- Use `Debug Helper` for runtime bugs, wrong output, or state-tracing work.
- Use `Security Specialist` for unsafe code review, secret scanning, and security-sensitive changes.

## Scope

- These skills are vendored from `udapy/rust-agentic-skills`, but only the Rust-focused parts needed for this project are included.
- Do not import or mirror the upstream repo's top-level `AGENTS.md` workflow gates here.
- Follow this repository's instructions in `AGENTS.md` first.

## Project Layout Reminder

- Cargo project root: `ImplicitDraft-src/`
- Build artifacts: `ImplicitDraft-output/`
- Scratch files: `ImplicitDraft-tmp/`
