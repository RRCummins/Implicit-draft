# Implicit Standalone Architecture

Created: 2026-03-29

## Scope

This document defines the standalone macOS app architecture for Implicit.

The repository now has two product surfaces:

- `Standalone`: native macOS app built with AppKit
- `CLI`: Rust terminal-first editor and utility tool

These two surfaces evolve in parallel, but they are not the same application with different skins.

## Product Boundaries

### Standalone

The standalone app is:

- macOS only
- AppKit-based
- single-window first
- tab-oriented
- sidebar-driven
- preview-driven with HTML/WebKit
- updateable from inside the app

The standalone app should feel like a native Mac document product, not a wrapped terminal UI.

### CLI

The CLI remains:

- terminal-first
- automation-friendly
- export-friendly
- useful without the standalone app

The CLI is the right place to keep utility leadership and shell-native workflows.

## Shared Behavior, Separate Implementations

The standalone app and CLI should stay aligned in behavior where it matters:

- file handling
- save semantics
- markdown meaning
- export semantics
- version/update semantics

But the implementations can diverge:

- preview rendering may be separate
- layout systems are separate
- interaction surfaces are separate

For preview specifically:

- standalone uses richer `HTML + WebKit`
- CLI stays visually aligned, but does not need to share the same renderer

## Standalone Structure

Current root:

- `ImplicitDraft-src/macos-project/implicit/`

Recommended organization as the app grows:

- `implicit/`
  - app entry and top-level window coordination
- `implicit/App/`
  - app delegate
  - menu wiring
  - update/install flow
- `implicit/Documents/`
  - document model
  - tab/session state
  - recent files
- `implicit/Editor/`
  - source editor
  - selection/find/replace
- `implicit/Preview/`
  - WebKit preview pipeline
  - HTML/CSS templates
- `implicit/Sidebar/`
  - tabs
  - recents
  - project navigation
- `implicit/Theme/`
  - visual tokens
  - typography
  - appearance rules
- `implicit/Export/`
  - print/export flows

Do not keep the whole app in one controller long-term.

## Core Standalone Principles

### 1. Tabs first

The first-class interaction model is:

- one window
- many tabs
- sidebar navigation

Not:

- one document per window as the main path

### 2. Preview is a first-class surface

Preview is not a fallback read-only text view.

It should become:

- a designed reading surface
- HTML/WebKit-backed
- visually aligned with the app chrome

### 3. Navigation is structural

The sidebar is not a dump of controls.

It should become the home for:

- tabs
- open documents
- recent files
- project/document navigation

### 4. Native app behavior matters

The standalone app should behave like a Mac app:

- Finder open
- recent docs
- drag/drop
- proper menus
- in-app update

## Versioning Strategy

Use one product version across both surfaces:

- CLI version and standalone app `MARKETING_VERSION` should track the same release version
- the Xcode project `MARKETING_VERSION` should match the tagged release version
- `CURRENT_PROJECT_VERSION` can stay as the internal build number for the standalone app

Release assets can differ, but the product version should not drift.

Expected release assets:

- CLI binary
- standalone app zip
- checksums

## Build Conventions

Standalone build:

```bash
xcodebuild -project ImplicitDraft-src/macos-project/implicit/implicit.xcodeproj -scheme implicit -configuration Debug -derivedDataPath ImplicitDraft-output/xcode-derived CODE_SIGNING_ALLOWED=NO build
```

Standalone output:

- `ImplicitDraft-output/xcode-derived/Build/Products/Debug/implicit.app`

CLI release output stays under:

- `ImplicitDraft-output/`

## Immediate Refactor Direction

The current standalone prototype is controller-heavy.

Next structural moves should be:

1. extract document/session state out of the main view controller
2. introduce a real tab model
3. introduce a dedicated preview layer with WebKit
4. move visual tokens into a dedicated theme/style surface

That is the base needed before the app can feel truly product-grade.
