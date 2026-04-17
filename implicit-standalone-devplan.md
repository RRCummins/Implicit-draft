# Implicit Standalone Dev Plan

Created: 2026-03-29
Updated: 2026-04-16
Scope: AppKit native macOS standalone app — Xcode project at `ImplicitDraft-src/macos-project/implicit/`.

---

## Product Direction

Implicit Standalone is a native macOS markdown editor built with AppKit. The design target is a Warp-like product: flat, dark, no decorative chrome, every pixel deliberate.

- Platform: **macOS only**
- Language: **Swift + AppKit**
- Architecture: single-window, multi-tab, `NSSplitView` workspace
- Preview: `WKWebView` HTML pipeline
- Distribution target: signed and notarized `.app` bundle

The standalone app is the primary product surface going forward. The CLI remains an independent tool.

Markdown target: **Obsidian-class markdown editing and preview.** The standalone app should get as close as practical to Obsidian’s markdown behavior, structure, and reading/writing comfort. Where choices differ, bias toward Obsidian conventions over inventing custom behavior.

---

## Design Language

All phases build toward a unified visual system. These tokens define it:

| Token | Value | Role |
|---|---|---|
| `bg` | `#0d1117` | main editor background |
| `surface` | `#161b22` | sidebar, panels |
| `chrome` | `#010409` | tab bar background |
| `tab-active` | `#0d1117` | active tab = matches editor (seamless) |
| `border` | `#30363d` | dividers, edges |
| `accent` | `#58a6ff` | active state, focus ring, cursor |
| `text-primary` | `#e6edf3` | body text |
| `text-muted` | `#8b949e` | labels, secondary info |
| `tab-bar-h` | `36pt` | |
| `sidebar-w` | `220pt` | |
| `status-bar-h` | `22pt` | |
| `body-font` | SF Pro Text 13pt | sidebar and status labels |
| `mono-font` | SF Mono 13pt | editor and code |

Window chrome: `titleVisibility = .hidden`, `titlebarAppearsTransparent = true`, `NSVisualEffectView` full-window background.

---

## Current Status

- `Phase 0` is complete.
- `Phase 1` is complete enough to build on.
- `Phase 2` is now in progress.
- `Phase 5` groundwork is now active through a real standalone markdown pipeline.
- Current committed checkpoints:
  - `fc3f8be` `feat: reset standalone app foundation`
  - `8abd21c` `feat: add standalone tab strip and start state`
  - `ff1c48a` `refactor: extract standalone markdown preview renderer`
  - `df27a99` `feat: improve standalone markdown blocks`
- Current local work after those commits is focused on:
  - document-owned editor state for tabs
  - safer open/save state transitions
  - moving controller-local document behavior into the session/model layer
  - replacing the lightweight preview parser with a real markdown rendering architecture
  - landing the first explicit markdown model/parser/HTML renderer split

## Markdown Architecture

**Goal:** Treat markdown as a first-class subsystem, not scattered string replacements.

### Core principles

- Source mode and preview mode must share the same document model, selection state, and block boundaries.
- Preview should be rendered from a structured markdown representation, not ad hoc regex substitutions.
- Editing conveniences should be driven by markdown structure: lists, task items, fences, blockquotes, tables, links, embeds.
- Obsidian is the product benchmark for:
  - markdown readability in source
  - stable preview structure
  - list and checkbox behavior
  - fenced code block handling
  - wikilinks, callouts, embeds, footnotes, and internal navigation

### Recommended architecture

**Layer 1 — Document text**
- Raw document text remains the source of truth.
- `NSTextView` owns text editing, undo, IME, selection, and typing behavior.

**Layer 2 — Markdown model**
- Add a dedicated markdown pipeline module, not more code inside `ViewController.swift`.
- Preferred structure:
  - `MarkdownParser.swift`
  - `MarkdownAST.swift`
  - `MarkdownHTMLRenderer.swift`
  - `MarkdownPreviewRenderer.swift`
  - `MarkdownEditBehavior.swift`
- Parser output should describe block structure:
  - headings
  - paragraphs
  - lists and nested lists
  - task list items
  - blockquotes
  - code fences with language
  - tables
  - thematic breaks
  - images and links
  - footnotes / callouts / embeds later

**Layer 3 — Preview HTML**
- Preview HTML should be generated from the markdown model, not directly from raw string replacements.
- HTML/CSS should aim visually toward Obsidian:
  - readable centered measure
  - strong table styling
  - proper code fence headers
  - callout blocks
  - image sizing and captions
  - polished blockquote and list spacing

**Layer 4 — Edit-mode markdown behaviors**
- Edit mode should gain markdown-aware behaviors from the same structure layer:
  - continue lists on Return
  - exit empty lists
  - indent/dedent nested lists
  - checkbox toggles
  - fence auto-close
  - heading-aware navigation
  - table row editing helpers

### Implementation rule

- Do not continue expanding the preview with one-off regex patches unless they are short-term stopgaps.
- New markdown work should move toward the dedicated parser / model / renderer split above.

### Current implementation checkpoint

The standalone app now has the first real cut of this structure:

- `MarkdownAST.swift`
- `MarkdownParser.swift`
- `MarkdownHTMLRenderer.swift`
- `MarkdownPreviewRenderer.swift`

The renderer is still intentionally lightweight compared to Obsidian, but preview is no longer just a controller-owned string-replacement pass.

## Current Baseline (Phase 0 Complete)

The Xcode project at `macos-project/implicit/` currently has:

- `AppDelegate.swift`: window sizing (1320×860), dark Aqua appearance, Finder open-file bridge, dock re-open behavior
- `ViewController.swift`:
  - `EditorDocument` struct: `id UUID`, `url URL?`, `title`, `text`, `isDirty`
  - `EditorMode` enum: `.source` / `.preview`
  - `NSSplitView` two-column layout
  - `NSTableView` document list sidebar (220pt fixed)
  - `NSTextView` editor in `NSScrollView`
  - `NSStackView` tab strip with active-document selection and `+` new-tab affordance
  - `NSSegmentedControl` source/preview mode switch
  - sidebar-owned new/open/save actions
  - `emptyStateView` start surface for untitled drafts
  - Dark monospace theme, caret amber `#E8CC91`

Phase 0 is done. This plan builds forward from here.

---

## Phase 1 — Visual System And App Shell

**Goal:** Replace prototype-era chrome with the full design language. Every region should feel intentional before any feature work happens.

### Tasks

**Palette and tokens:**
- Define `Palette` and `Metrics` enums in a dedicated `DesignSystem.swift` file
- Migrate all hardcoded colors and sizes in `ViewController.swift` to these enums
- Establish `NSColor` extensions for hex literals: `NSColor(hex: 0x0d1117)`

**Tab bar:**
- Replace the `NSStackView` tab strip with a proper `TabBarView: NSView`
- `TabBarView` draws its own background (`chrome` color, 36pt height)
- Active tab background `tab-active` color seamlessly blends into the editor
- Tab shows: title, dirty dot (•), close button (×) on hover
- No system tab bar — drawn entirely in `TabBarView`

**Sidebar:**
- Replace `NSTableView` document list with a structured sidebar that has named sections
- Sidebar header row: 28pt, section label in `text-muted`, no separator chrome
- Sidebar items: 32pt row height, left-padded 12pt, selection highlight uses `accent` at 15% opacity
- Right edge: 1pt `border` color divider, no shadow

**Status bar:**
- 22pt bottom bar, `surface` background
- Left: file path or mode label
- Right: word count, line/col, or mode indicator
- 1pt top edge border

**Window:**
- Full-window `NSVisualEffectView` in `.behindWindow` material as the base layer
- All subviews sit on top — no floating cards, no margins, no insets on the editor
- Editor NSTextView goes edge-to-edge against the sidebar divider

**Empty state:**
- No modal overlay — inline centered content in the editor region
- Two lines: large `text-muted` headline + smaller instruction
- Single "Open File" button, `accent`-colored, no border

### Acceptance

- App launches with a blank untitled state that looks designed, not scaffolded
- Tab bar, sidebar, editor, and status bar form one composed workspace
- No prototype-era color values or layout constants remain in code
- Dark Aqua appearance throughout, no light-mode artifacts

---

## Phase 2 — Document And Session Model

**Goal:** Replace controller-local document state with a real model layer. Documents and session state are separate concerns from the view.

### Tasks

**Document model — `Document.swift`:**
```swift
struct Document: Identifiable {
    let id: UUID
    var url: URL?
    var title: String
    var content: String
    var isDirty: Bool
    var scrollOffset: CGFloat
    var selectionRange: NSRange
    var mode: EditorMode
}
```

**Session model — `Session.swift`:**
```swift
class Session: ObservableObject {
    var documents: [Document]
    var activeID: UUID?
    var sidebarVisible: Bool
    var sidebarSection: SidebarSection
    func save()       // persist to UserDefaults or JSON on disk
    func restore()    // load on launch
}
```

**Persistence:**
- Encode session to `~/.config/implicit/session.json` (or `Application Support`)
- Save on: tab close, app resign active, window close
- Restore on: app launch, before first draw
- Dirty-document recovery: autosave scratch copies to `Application Support/Implicit/Recovery/`

**Close behavior:**
- Closing a dirty tab shows `NSAlert` with "Save", "Don't Save", "Cancel"
- Closing the window with multiple dirty tabs: iterate each, confirm in sequence
- `applicationShouldTerminate`: return `.terminateLater`, resolve all dirty confirms, then proceed

**Recent files:**
- Track last 20 opened URLs in `UserDefaults`
- Expose as `recentURLs: [URL]` on `Session`

### Acceptance

- Open tabs and their content survive a normal restart
- Dirty state is always accurate — no false "clean" on unsaved content
- Close/reopen behavior is predictable and matches macOS conventions
- App never loses content across a crash or force-quit (recovery files exist)

---

## Phase 3 — Tabs First

**Goal:** First-class single-window multi-tab workflow. Tabs are the primary document surface; sidebar is navigation.

### Tasks

**Tab model:**
- `TabBarView` is driven entirely by `Session.documents`
- Tab creation paths:
  - `⌘N` — new untitled document, opens in new tab
  - `⌘O` — open file dialog, result opens in new tab
  - `⌘T` — alias for new tab
  - Sidebar row double-click — opens in new tab, or activates if already open
- Tab close: `⌘W` closes active tab; button click closes that tab
- Tab reordering: `NSDraggingSource`/`NSDraggingDestination` in `TabBarView`
- Tab overflow: when tabs exceed bar width, show left/right scroll arrows or compact
- `⌘{` / `⌘}` — previous/next tab

**Sidebar sync:**
- "Open" section in sidebar reflects open tabs
- "Recent" section reflects `session.recentURLs`
- Selecting a sidebar item activates or opens the document
- Active tab is highlighted in sidebar "Open" section

### Acceptance

- The main daily workflow is single-window multi-tab use with no Finder dependency
- Tab creation, switching, and close all work without unexpected state loss
- Sidebar accurately reflects open and recent documents

---

## Phase 4 — Source Editor Core

**Goal:** NSTextView configuration that makes markdown writing comfortable.

### Tasks

**NSTextView setup:**
- Font: SF Mono 13pt
- Background: `bg` color
- Text color: `text-primary`
- Caret color: `accent`
- Line height: 1.5× via `NSParagraphStyle`
- Disable autocorrect, autocapitalize, smart quotes, smart dashes
- Horizontal scroll disabled — word wrap at content width
- Container inset: 24pt horizontal, 20pt vertical

**Behavior:**
- Undo/redo: `NSTextView` built-in; confirm it survives tab switches
- Home/End: move to line start/end (not document)
- `⌘↑` / `⌘↓`: jump to document top/bottom
- Selection: standard, plus `⌥Click` column select if feasible
- Find: `⌘F` activates `NSTextFinder` bar inline — do not use a separate modal
- Go to line: `⌃G` or `⌘L` — custom minimal popover input

**File safety:**
- External file change detection: `DispatchSource.makeFileSystemObjectSource`
- On change: banner alert "File changed externally — Reload or Keep Mine?"
- Save shortcut: `⌘S`; Save As: `⌘⇧S`

**Status bar:**
- Live word count
- Line and column numbers derived from current cursor position
- Update on every `textDidChange` and selection change

### Acceptance

- Normal markdown writing is comfortable — no lag, no jarring autocorrections
- Undo, find, and jump all work as expected
- Word count and line/col are always accurate

---

## Phase 5 — Preview Engine

**Goal:** First-class HTML preview via WKWebView with Obsidian-level markdown coverage.

### Tasks

**WKWebView pipeline:**
- Replace `NSTextView` preview fallback with a `WKWebView` in a `PreviewViewController`
- `PreviewViewController` is swapped in/out based on active tab's `mode`
- Preview receives: document content (markdown string), theme tokens, mode

**Markdown renderer:**
- Replace the lightweight string-replacement renderer with a dedicated parser / model / renderer stack
- Prefer `cmark-gfm` via Swift Package Manager if integration stays clean; otherwise keep a custom parser only if it can grow into a real markdown block model
- Support parity target:
  - headings, paragraphs, bold, italic, links, images
  - ordered and unordered lists
  - nested lists
  - task lists
  - blockquotes
  - tables
  - fenced code blocks with language labels
  - inline code
  - strikethrough
  - footnotes
  - callouts
  - internal links / wikilinks
  - code span escaping and fence edge cases

**Preview CSS system:**
- Embed a `preview.css` in the bundle
- CSS uses the same design tokens as the Swift shell, but the document surface should feel closer to Obsidian than GitHub
- Body max-width: 760pt centered, `body-font`
- Code blocks: `surface` background, SF Mono, syntax-highlighted via bundled highlighter
- Tables: horizontally scrollable wrapper, sticky visual hierarchy, alignment-aware cells
- Callouts: Obsidian-like block treatment
- No external network requests from preview

**Sync:**
- Preview updates on every `textDidChange` with 200ms debounce
- Scroll position is preserved across content updates (inject JS `scrollTop` restore)
- Switching source↔preview is instant — no flash or blank frame

### Acceptance

- Preview reads like a designed document, not a fallback renderer
- Source/preview switching is immediate and stable for documents up to ~100KB
- Preview CSS is visually consistent with the rest of the app
- Tables, task lists, code fences, images, and blockquotes feel production quality
- The preview is credible next to Obsidian, not merely “good enough”

---

## Phase 6 — Sidebar And Navigation

**Goal:** Sidebar becomes a real navigation surface, not just a document list.

### Tasks

**Sidebar sections:**
- `Open` — currently open tabs, sorted by open time
- `Recent` — last 20 files, sorted by recency
- `Project` — file tree if a folder is open (optional, can ship later)

**Section implementation:**
- Custom `NSOutlineView` or `NSTableView` with section headers
- Section headers: 28pt, uppercase 10pt label in `text-muted`, no disclosure triangle
- Row height: 32pt

**File operations (right-click context menu):**
- Rename (inline edit)
- Delete (move to Trash via `FileManager`)
- Reveal in Finder
- Copy path

**New file in sidebar:**
- "+" button in sidebar header creates new untitled document in the active folder

**Quick open:**
- `⌘P` — command palette popover
- Fuzzy search across open tabs and recent files
- Keyboard navigation (↑↓ + Return)
- Appears centered in editor region, dismissed on Escape or selection

**Drag and drop:**
- Drag a file from Finder onto the sidebar → open in new tab
- Drag a file onto the editor → insert file path or inline content

### Acceptance

- Opening, switching, and closing documents never requires Finder
- Quick open (`⌘P`) finds recent and open files in under two keystrokes
- Sidebar accurately reflects open and recent state at all times

---

## Phase 7 — Writing Features

**Goal:** Close the gap on everyday editing workflows.

### Tasks

**Find and replace:**
- `⌘F` — find bar (NSTextFinder)
- `⌘⌥F` — show replace field
- Match count displayed
- Regex toggle
- Replace / Replace All
- Search result highlighting in editor

**Markdown conveniences:**
- Tab key in a list item: indent
- Return at end of list item: continue list with same prefix (`-`, `*`, `1.`)
- Return after empty list item: dedent and exit list
- `- [ ]` checkbox: click to toggle in source (or in preview)
- Triple backtick + Return: auto-close code fence with closing backticks
- `**` / `_` wrapping: select text, type delimiter → wrap selection
- Table editing helpers:
  - Return adds next row when cursor is in a table row
  - Tab / Shift-Tab moves across cells
  - alignment separator row inserted correctly
- Obsidian-style markdown behaviors where practical:
  - wikilink insertion and navigation
  - callout syntax helpers
  - footnote insertion
  - heading folding later if feasible

**Word count and stats:**
- Word count in status bar (live)
- Estimated reading time (optional, popover or tooltip)

### Acceptance

- Writing, editing, and revising markdown never feels broken or unfinished
- List continuation and code fence behavior matches common editors (VS Code, Typora)

---

## Phase 8 — Code And Technical Writing

**Goal:** Implicit should be strong for README, API docs, and mixed markdown/code workflows.

### Tasks

**Syntax highlighting in source view:**
- Use `NSTextStorage` + `NSLayoutManager` subclass for syntax highlighting
- Highlight: headings (`#`), bold/italic, code spans, fenced code block language label, links, blockquotes
- Colors from the design palette: headings in `text-primary` at full weight, code spans in `surface`-tinted background, links in `accent`

**Code file support:**
- Detect non-markdown files (`.swift`, `.rs`, `.py`, `.json`, `.yaml`, etc.) by extension
- Open in source mode with monospace font; no markdown conveniences applied
- No syntax highlighting for code files in v1 (defer)

**Line numbers:**
- Optional left gutter, off by default
- Toggle via `View > Show Line Numbers` or status bar click
- Gutter: 44pt wide, `text-muted`, right-aligned

**Git change indicators (stretch):**
- Check for `.git` folder when opening a file
- Show added/modified/removed line indicators in gutter margin
- Defer if it adds significant complexity

### Acceptance

- Heading and inline code highlighting make source view readable, not just a plain textarea
- Line numbers work correctly and don't break layout at any window size

---

## Phase 9 — Native macOS Integration

**Goal:** Implicit behaves like a first-class Mac document app.

### Tasks

**Document type associations:**
- `Info.plist`: register `.md` and `.markdown` as handled UTIs
- `CFBundleDocumentTypes` with `LSHandlerRank = Default`
- Icon for document type (defer custom icon)

**App menu polish:**
- `File` menu: New, Open, Open Recent (using `NSDocumentController.shared.noteNewRecentDocumentURL`), Close, Save, Save As, Revert
- `Edit` menu: standard undo/redo/cut/copy/paste + Find submenu
- `View` menu: Toggle Sidebar, Toggle Preview, Show Line Numbers
- `Window` menu: standard macOS window management
- Remove any placeholder or debug menu items

**Open Recent:**
- `NSDocumentController.shared.noteNewRecentDocumentURL(url)` on every file open
- "Open Recent" submenu auto-populated by AppKit

**Services:**
- Register text service for "New Implicit Document from Selection" (stretch)

**Dock menu:**
- `applicationDockMenu`: add "New Document" item

**Preferences window:**
- `⌘,` opens a minimal preferences window
- Settings: font size, line height, word wrap width, auto-save interval, startup behavior (restore session / open new)

### Acceptance

- Double-clicking a `.md` file in Finder opens it in Implicit
- Open Recent works identically to other Mac document apps
- App menu is complete, correct, and has no debug-era items

---

## Phase 10 — Export And Utility Flows

**Goal:** Export is trustworthy and covers the common cases.

### Tasks

**Export HTML:**
- `File > Export > HTML…` — save panel
- Writes the same HTML the preview renders, with embedded CSS

**Export PDF:**
- Use `WKWebView.printOperation(with:)` → print to PDF
- Default destination: same directory as source file

**Print:**
- `⌘P` in non-find context → `NSPrintOperation` via WKWebView
- Print preview shows the same rendered markdown, not the source text

**Selection export:**
- Select text in source → `File > Export > Selection as HTML…`
- Only exports the selected range, rendered as HTML

**Export presets (stretch):**
- Save export settings (paper size, margins, CSS overrides) as named presets

### Acceptance

- HTML export produces a self-contained file that renders correctly in Safari and Chrome
- PDF export produces a paginated document that resembles the preview
- Print dialog works without crashes on current macOS

---

## Phase 11 — Auto-Update And Release Flow

**Goal:** The app can deliver updates to users without requiring a CLI or manual download.

### Tasks

**Update channel:**
- Host releases on GitHub Releases (or a custom endpoint)
- Release asset: `Implicit-{version}.zip` containing the signed `.app`
- `appcast.xml` or a JSON version manifest at a fixed URL

**In-app update check:**
- On launch (once per day): fetch version manifest, compare with bundle version
- If update available: banner notification at top of window with "Update Available" + version
- `Help > Check for Updates…` for manual check

**Update download and apply:**
- Download zip to `~/Library/Caches/Implicit/`
- Verify SHA-256 checksum against manifest
- Extract, replace app in `/Applications`, relaunch
- Use `LaunchServices` for relaunch; handle sandboxing constraints

**In-app version display:**
- `Help > About Implicit` shows version, build number, and a link to release notes
- Version from `CFBundleShortVersionString`, build from `CFBundleVersion`

**Release notes:**
- Release notes URL embedded in manifest
- "What's New" view: simple `WKWebView` loading release notes page

### Acceptance

- User can receive and apply an update entirely within the app
- Update process never silently corrupts the installed app
- Version is always visible and correct

---

## Phase 12 — Hardening, Signing, And Notarization

**Goal:** Repeatable signed and notarized release pipeline. App is stable enough for daily use.

### Tasks

**Stability:**
- Test session restore with 10+ open tabs
- Test large file behavior (≥1MB markdown)
- Test with files on iCloud Drive, network volumes
- Confirm undo history is bounded (no unbounded memory growth)
- Confirm no main-thread file I/O (all disk access on background queues)

**Startup performance:**
- Cold launch to first paint: target ≤400ms
- Session restore should not block the main thread

**Signing:**
- Developer ID Application certificate
- Hardened runtime enabled
- Entitlements: `com.apple.security.files.user-selected.read-write`, `com.apple.security.network.client` (for update check)

**Notarization:**
- `xcrun notarytool submit` pipeline
- Staple notarization ticket: `xcrun stapler staple`
- Automate in `macos/archive_app.sh` or a new `release_app.sh` script

**QA checklist:**
- [ ] Open, edit, save new file
- [ ] Open, edit, save existing file
- [ ] Dirty-state warning on close
- [ ] Session restore across restart
- [ ] Finder double-click open
- [ ] Drag file onto dock icon
- [ ] Export HTML and PDF
- [ ] Update check and apply flow
- [ ] First launch on a clean system (no `~/.config/implicit`)

### Acceptance

- Signed and notarized `.app` passes Gatekeeper on a clean Mac
- No known crashes in the QA checklist flows
- `archive_app.sh` produces a distributable zip with no manual steps

---

## Recommended Phase Order

```
1  → Phase 1  (Visual System)
2  → Phase 2  (Document + Session Model)
3  → Phase 3  (Tabs First)
4  → Phase 5  (Preview Engine)
5  → Phase 4  (Source Editor Core)     ← after tabs, before writing features
6  → Phase 6  (Sidebar + Navigation)
7  → Phase 7  (Writing Features)
8  → Phase 9  (Native macOS Integration)
9  → Phase 8  (Code + Technical Writing)
10 → Phase 10 (Export)
11 → Phase 11 (Auto-Update)
12 → Phase 12 (Hardening + Signing)
```

Preview (Phase 5) comes before full editor polish (Phase 4) because it defines the app's identity — users need to see that the preview is real before the editing experience is complete.

---

## Immediate Next Slice

**Phase 1 tasks to start now:**

1. Create `DesignSystem.swift` — all palette and metric constants, `NSColor(hex:)` extension
2. Implement `TabBarView` — custom `NSView`, 36pt, drawn background, tab items as subviews or drawn directly
3. Replace `NSTableView` sidebar with the three-section structure (`Open`, `Recent`, `Project`)
4. Add `StatusBarView` — 22pt, `surface` background, word count left, line/col right
5. Strip all prototype-era inline constants from `ViewController.swift`
