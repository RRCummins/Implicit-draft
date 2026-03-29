# Implicit Standalone Dev Plan

Created: 2026-03-29  
Scope: Restart the standalone app plan from the start of the Xcode/AppKit project while continuing CLI/TUI development in parallel.

## Product Direction

- The project now has two active product surfaces:
  - `Standalone`: native macOS app built with AppKit
  - `CLI`: Rust terminal-first editor and utility tool
- Both tracks continue in parallel.
- Platform scope for standalone is `macOS only`.
- Standalone architecture target is:
  - single-window app first
  - tabs and sidebar inside that window
  - richer HTML/WebKit preview
  - Warp-like product chrome
  - built-in auto-update
- Release target can include signing and notarization.

## Plan Principles

- The standalone app is not a thin wrapper around the CLI.
- The CLI remains a first-class product surface, especially for terminal-first workflows, automation, export, and utility flows.
- Shared behavior should stay aligned across both surfaces where it matters:
  - file handling
  - markdown semantics
  - export behavior
  - theme direction
  - update/version semantics
- The standalone app should feel like a native Mac product, not a terminal app embedded in a window.

## Current Baseline

The Xcode/AppKit app currently has:

- one window
- one controller-driven layout
- basic source and preview modes
- open/save/save-as/revert
- Finder open-file handling
- a document list sidebar

This plan treats that as the Phase 0 baseline and builds forward from there.

## Phase 0 — Foundation Reset

Goal: make the Xcode project a stable base before feature expansion.

Tasks:

- [x] define repo conventions for the standalone app under `ImplicitDraft-src/macos-project/implicit/`
- [x] clean Xcode user-state noise from source control
- [x] stabilize target settings:
  - deployment target
  - bundle id
  - product name
  - version/build strategy
- [x] add a small architecture note for the standalone app
- [x] define shared product vocabulary across app and CLI:
  - source
  - preview
  - tabs
  - sidebar
  - export
  - update

Acceptance:

- [x] Xcode project builds cleanly from command line
- [x] no user-specific Xcode files need to be committed
- [x] app bundle naming/versioning is deterministic

## Phase 1 — App Shell And Visual System

Goal: replace the current prototype feel with a real product shell.

Tasks:

- define visual tokens:
  - colors
  - spacing
  - radii
  - borders
  - typography
- simplify top chrome into a durable structure:
  - app identity
  - active tab strip
  - mode switching
  - utility actions
- redesign the sidebar as navigation instead of a generic panel
- define empty-state/start-screen behavior
- define status surface behavior
- make preview, editor, sidebar, and header feel like one composed workspace

Acceptance:

- untitled launch feels intentional
- no prototype-looking regions remain
- clear hierarchy between chrome, navigation, and editor content

## Phase 2 — Document And Session Model

Goal: move from controller-local document state to a real app document/session model.

Tasks:

- formal document model for:
  - id
  - url
  - title
  - dirty state
  - editor state
  - preview state
- session model for:
  - open tabs
  - selected tab
  - sidebar state
  - window state
- safe close behavior
- recent files
- reopen last session
- dirty-document restore strategy

Acceptance:

- open tabs survive normal restart
- dirty state is accurate
- close/reopen behavior is predictable

## Phase 3 — Tabs First

Goal: deliver the first-class single-window tab workflow.

Tasks:

- real tab model, not one document per sidebar row
- tab creation:
  - new untitled
  - open file in new tab
  - duplicate tab
- tab closing with dirty-confirm behavior
- tab reordering
- tab overflow behavior
- synchronize sidebar with tabs and recent/open documents

Acceptance:

- the main workflow is single-window multi-tab use
- tabs feel primary, sidebar feels secondary/navigation-oriented

## Phase 4 — Source Editor Core

Goal: make the standalone editor credible as a daily writing and editing surface.

Tasks:

- robust `NSTextView` configuration or custom text system improvements
- line/column status
- undo/redo
- selection handling
- Home/End and document jump behavior
- find
- goto line
- save safety
- external file change detection
- file type detection

CLI parallel tasks:

- keep source-mode behavior aligned where it makes sense
- keep key movement and save semantics consistent

Acceptance:

- normal markdown writing/editing is comfortable
- no common editing flow feels broken or placeholder

## Phase 5 — Preview Engine

Goal: ship the first real standalone preview using HTML/WebKit.

Tasks:

- introduce `WKWebView` preview pipeline
- define preview renderer inputs:
  - current document text
  - theme
  - mode
  - metadata
- support:
  - headings
  - lists
  - task lists
  - links
  - blockquotes
  - tables
  - fenced code blocks
  - inline code
- add preview CSS system
- align preview styling direction with the standalone visual system
- preserve source/preview mode switching cleanly per tab

CLI parallel tasks:

- continue aligning markdown semantics and export semantics
- reuse markdown rules where practical

Acceptance:

- preview reads like a designed document view, not a fallback renderer
- switching between source and preview is immediate and stable

## Phase 6 — Sidebar And Navigation

Goal: make navigation fast and product-grade.

Tasks:

- restructure sidebar into sections such as:
  - open tabs
  - recent files
  - project files
- add quick open / command palette
- add project/file browser mode
- add file operations:
  - rename
  - delete
  - new file
  - new folder
- preserve sidebar selection state
- refine drag/drop open behavior

CLI parallel tasks:

- continue deeper sidebar/file-ops parity where relevant

Acceptance:

- opening and switching documents is fast without using Finder
- sidebar feels like a real navigation surface

## Phase 7 — Writing Features

Goal: close the gap on everyday editor workflows.

Tasks:

- find and replace
- regex option
- search result highlighting
- replace all
- split editor and preview if still justified
- markdown conveniences:
  - list continuation
  - smart checkbox handling
  - code fence helpers

CLI parallel tasks:

- keep replace/search semantics aligned

Acceptance:

- writing, editing, and revising documents does not require dropping back to the CLI

## Phase 8 — Code And Technical Writing Support

Goal: make Implicit strong for docs-plus-code workflows.

Tasks:

- syntax highlighting strategy for source files
- code file mode defaults
- code-aware preview/code block treatment
- git change indicators
- conflict marker visibility
- line numbers
- technical-document authoring polish

CLI parallel tasks:

- continue shared syntax/export/theme direction

Acceptance:

- the app feels strong for README, API docs, and mixed markdown/code work

## Phase 9 — Native macOS Integration

Goal: make the standalone app behave like a polished Mac app.

Tasks:

- Finder `Open With`
- document type associations
- drag/drop polish
- app menu polish
- `Open Recent`
- Services integration if useful
- proper `implicit --new-window` bridge into the app
- app-level settings/preferences window

Acceptance:

- users can treat Implicit as a normal Mac document app

## Phase 10 — Export And Utility Flows

Goal: bring the strong CLI export story into the standalone app.

Tasks:

- export HTML
- export PDF
- export snapshot/image
- print
- selection export
- export presets
- export dialog with format-specific messaging

CLI parallel tasks:

- keep the CLI export feature set ahead or equal where practical

Acceptance:

- standalone export is trustworthy and coherent with the CLI

## Phase 11 — Auto-Update And Release Flow

Goal: make the standalone app independently installable and updateable.

Tasks:

- define update channel strategy
- implement built-in update check UI
- implement in-app update apply flow
- define release asset layout:
  - app zip
  - CLI binary
  - checksums
- version/build presentation in the app
- release notes surface

Acceptance:

- users can install and update the standalone app without using the CLI

## Phase 12 — Hardening, Signing, And Notarization

Goal: move from dev build to shippable Mac app.

Tasks:

- crash and recovery review
- large-file behavior
- startup/perf profiling
- memory review for long sessions
- notarization flow
- signed release flow
- QA checklist for file handling and session restore

Acceptance:

- repeatable signed and notarized release pipeline exists
- app is stable enough for real use

## Parallel CLI Track

The CLI stays active during the standalone build-out.

CLI priorities during the standalone phases:

- keep core editor quality improving
- preserve export and utility leadership
- remain the fastest automation-friendly entry point
- continue shared markdown and file-behavior alignment
- expose app-launch bridges:
  - `implicit --new-window`
  - future app handoff flows

The CLI should not be frozen while the standalone app matures.

## Recommended Immediate Order

1. Phase 0
2. Phase 1
3. Phase 2
4. Phase 3
5. Phase 5
6. Phase 6
7. Phase 7
8. Phase 8
9. Phase 9
10. Phase 10
11. Phase 11
12. Phase 12

Rationale:

- tabs and document/session state have to come before deeper editor and preview polish
- preview should happen early because it defines the standalone app’s identity
- auto-update should land before the final release hardening pass, not after

## Immediate Next Slice

Start with `Phase 0` and finish it completely:

- clean Xcode git noise
- lock the standalone app structure
- add a lightweight standalone architecture note
- define versioning/release expectations for app plus CLI

Then move directly into `Phase 1` with a real shell pass:

- top chrome
- tab strip
- sidebar hierarchy
- empty state
- status surface
