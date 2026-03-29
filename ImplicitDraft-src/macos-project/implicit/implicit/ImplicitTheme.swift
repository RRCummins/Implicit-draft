// ImplicitTheme.swift
// Shared editor theme system. Field names mirror the CLI's ThemeFile TOML keys exactly,
// enabling shared ~/.config/implicit/themes/*.toml files across both surfaces.

import Cocoa

// MARK: - Theme struct

struct ImplicitTheme {
    // Headings
    var heading1: NSColor
    var heading2: NSColor
    var heading3: NSColor
    var heading4: NSColor
    var heading5: NSColor
    var heading6: NSColor
    // Inline styles
    var bold: NSColor
    var italic: NSColor
    // Code
    var code: NSColor         // inline code foreground
    var codeBg: NSColor       // inline + fenced code background (TOML key: code_bg)
    var codeKeyword: NSColor  // TOML key: code_keyword
    var codeString: NSColor   // TOML key: code_string
    var codeComment: NSColor  // TOML key: code_comment
    var codeNumber: NSColor   // TOML key: code_number
    var codeType: NSColor     // TOML key: code_type
    var codePunctuation: NSColor  // TOML key: code_punctuation
    // Block elements
    var blockquote: NSColor
    var link: NSColor
    var rule: NSColor
    var conflictMarker: NSColor  // TOML key: conflict_marker
    var listMarker: NSColor      // TOML key: list_marker
    // UI chrome (section headers, active labels)
    var uiChrome: NSColor        // TOML key: ui_chrome
    // Git indicators
    var gitAdded: NSColor        // TOML key: git_added
    var gitDeleted: NSColor      // TOML key: git_deleted
    var gitModified: NSColor     // TOML key: git_modified
    var gitUntracked: NSColor    // TOML key: git_untracked
    // Editor state
    var cursor: NSColor          // cursor background (bg role in CLI)
    var selection: NSColor       // selection background (bg role in CLI)
    var background: NSColor      // editor background
}

extension ImplicitTheme {
    func heading(level: Int) -> NSColor {
        switch level {
        case 1: heading1
        case 2: heading2
        case 3: heading3
        case 4: heading4
        case 5: heading5
        default: heading6
        }
    }
}

// MARK: - Built-in themes

extension ImplicitTheme {
    /// All built-in theme names — matches the CLI's BUILTIN_THEME_NAMES list.
    static let builtinNames: [String] = [
        "dark", "light",
        "gruvbox", "gruvbox-dark", "gruvbox-light",
        "catppuccin-mocha", "catppuccin-latte",
        "dracula", "nord", "one-dark", "rose-pine",
        "solarized-dark", "tokyo-night",
    ]

    /// Returns the named built-in theme, or nil if the name is not recognized.
    static func builtin(named name: String) -> ImplicitTheme? {
        switch name {
        case "dark":
            // GitHub dark palette — the visual reference for the standalone app.
            // The CLI uses terminal colors for "dark"; these are the AppKit equivalents.
            return make(
                h: (0x79c0ff, 0xd2a8ff, 0x7ee787, 0xe3b341, 0xf78166, 0x8b949e),
                codeFg: 0xe6edf3, codeBg: 0x161b22,
                quote: 0x8b949e, link: 0x58a6ff, rule: 0x30363d,
                list: 0x7ee787, chrome: 0x58a6ff,
                cursor: 0x2a3a4c, sel: 0x1f3a5c, bg: 0x0d1117
            )
        case "light":
            return make(
                h: (0x0550ae, 0x8250df, 0x116329, 0x953800, 0xcf222e, 0x57606a),
                codeFg: 0x24292f, codeBg: 0xf6f8fa,
                quote: 0x57606a, link: 0x0969da, rule: 0xd0d7de,
                list: 0x116329, chrome: 0x0550ae,
                cursor: 0xdce6f2, sel: 0xdce6f2, bg: 0xffffff
            )
        case "gruvbox", "gruvbox-dark":
            return make(
                h: (0xfabd2f, 0x83a598, 0xb8bb26, 0xd3869b, 0xfb4934, 0x928374),
                codeFg: 0xebdbb2, codeBg: 0x3c3836,
                quote: 0xa89984, link: 0x83a598, rule: 0x504945,
                list: 0xb8bb26, chrome: 0xfabd2f,
                cursor: 0x45403d, sel: 0x3c3836, bg: 0x282828
            )
        case "gruvbox-light":
            return make(
                h: (0xaf3a03, 0x076678, 0x79740e, 0x8f3f71, 0x9d0006, 0x928374),
                codeFg: 0x3c3836, codeBg: 0xfbf1c7,
                quote: 0x7c6f64, link: 0x076678, rule: 0xd5c4a1,
                list: 0x79740e, chrome: 0xaf3a03,
                cursor: 0xebdbb2, sel: 0xefe3bc, bg: 0xfbf1c7
            )
        case "catppuccin-mocha":
            return make(
                h: (0x89b4fa, 0x74c7ec, 0xa6e3a1, 0xf9e2af, 0xf5c2e7, 0xbac2de),
                codeFg: 0xcdd6f4, codeBg: 0x313244,
                quote: 0xa6adc8, link: 0x89b4fa, rule: 0x585b70,
                list: 0xa6e3a1, chrome: 0x94e2d5,
                cursor: 0x45475a, sel: 0x313244, bg: 0x1e1e2e
            )
        case "catppuccin-latte":
            return make(
                h: (0x1e66f5, 0x04a5e5, 0x40a02b, 0xdf8e1d, 0xea76cb, 0x6c6f85),
                codeFg: 0x4c4f69, codeBg: 0xdce0e8,
                quote: 0x7c7f93, link: 0x1e66f5, rule: 0xacb0be,
                list: 0x40a02b, chrome: 0x1e66f5,
                cursor: 0xdce0e8, sel: 0xccd0da, bg: 0xeff1f5
            )
        case "dracula":
            return make(
                h: (0x8be9fd, 0x50fa7b, 0xf1fa8c, 0xffb86c, 0xff79c6, 0xbd93f9),
                codeFg: 0xf8f8f2, codeBg: 0x44475a,
                quote: 0x6272a4, link: 0x8be9fd, rule: 0x6272a4,
                list: 0x50fa7b, chrome: 0xff79c6,
                cursor: 0x44475a, sel: 0x44475a, bg: 0x282a36
            )
        case "nord":
            return make(
                h: (0x88c0d0, 0x81a1c1, 0xa3be8c, 0xebcb8b, 0xb48ead, 0xd8dee9),
                codeFg: 0xe5e9f0, codeBg: 0x434c5e,
                quote: 0x81a1c1, link: 0x88c0d0, rule: 0x4c566a,
                list: 0xa3be8c, chrome: 0x8fbcbb,
                cursor: 0x3b4252, sel: 0x434c5e, bg: 0x2e3440
            )
        case "one-dark":
            return make(
                h: (0x61afef, 0x56b6c2, 0x98c379, 0xe5c07b, 0xc678dd, 0xabb2bf),
                codeFg: 0xabb2bf, codeBg: 0x282c34,
                quote: 0x5c6370, link: 0x61afef, rule: 0x5c6370,
                list: 0x98c379, chrome: 0xc678dd,
                cursor: 0x313640, sel: 0x3e4451, bg: 0x282c34
            )
        case "rose-pine":
            return make(
                h: (0xc4a7e7, 0x9ccfd8, 0xebbcba, 0xf6c177, 0xeb6f92, 0xe0def4),
                codeFg: 0xe0def4, codeBg: 0x26233a,
                quote: 0x908caa, link: 0x9ccfd8, rule: 0x393552,
                list: 0xf6c177, chrome: 0xc4a7e7,
                cursor: 0x312c3f, sel: 0x26233a, bg: 0x191724
            )
        case "solarized-dark":
            return make(
                h: (0x268bd2, 0x2aa198, 0x859900, 0xb58900, 0xd33682, 0x93a1a1),
                codeFg: 0x93a1a1, codeBg: 0x073642,
                quote: 0x586e75, link: 0x268bd2, rule: 0x586e75,
                list: 0x859900, chrome: 0x2aa198,
                cursor: 0x002b36, sel: 0x073642, bg: 0x002b36
            )
        case "tokyo-night":
            return make(
                h: (0x7aa2f7, 0x7dcfff, 0x9ece6a, 0xe0af68, 0xbb9af7, 0xc0caf5),
                codeFg: 0xc0caf5, codeBg: 0x24283b,
                quote: 0x565f89, link: 0x7aa2f7, rule: 0x414868,
                list: 0x9ece6a, chrome: 0x7dcfff,
                cursor: 0x292e42, sel: 0x24283b, bg: 0x1a1b26
            )
        default:
            return nil
        }
    }

    // Mirrors the CLI's `from_palette` function.
    private static func make(
        h: (UInt32, UInt32, UInt32, UInt32, UInt32, UInt32),
        codeFg: UInt32, codeBg: UInt32,
        quote: UInt32, link: UInt32, rule: UInt32,
        list: UInt32, chrome: UInt32,
        cursor: UInt32, sel: UInt32, bg: UInt32
    ) -> ImplicitTheme {
        ImplicitTheme(
            heading1:        NSColor(rgb: h.0),
            heading2:        NSColor(rgb: h.1),
            heading3:        NSColor(rgb: h.2),
            heading4:        NSColor(rgb: h.3),
            heading5:        NSColor(rgb: h.4),
            heading6:        NSColor(rgb: h.5),
            bold:            NSColor(rgb: codeFg),
            italic:          NSColor(rgb: codeFg),
            code:            NSColor(rgb: codeFg),
            codeBg:          NSColor(rgb: codeBg),
            codeKeyword:     NSColor(rgb: link),
            codeString:      NSColor(rgb: codeFg),
            codeComment:     NSColor(rgb: quote),
            codeNumber:      NSColor(rgb: h.3),
            codeType:        NSColor(rgb: h.1),
            codePunctuation: NSColor(rgb: chrome),
            blockquote:      NSColor(rgb: quote),
            link:            NSColor(rgb: link),
            rule:            NSColor(rgb: rule),
            conflictMarker:  NSColor(rgb: 0xff4444),
            listMarker:      NSColor(rgb: list),
            uiChrome:        NSColor(rgb: chrome),
            gitAdded:        NSColor(rgb: 0x3fb950),
            gitDeleted:      NSColor(rgb: 0xf85149),
            gitModified:     NSColor(rgb: 0xd29922),
            gitUntracked:    NSColor(rgb: 0x8b949e),
            cursor:          NSColor(rgb: cursor),
            selection:       NSColor(rgb: sel),
            background:      NSColor(rgb: bg)
        )
    }
}

// MARK: - Loader

/// Loads the theme selected in `~/.config/implicit/config.toml`, applying
/// any user overrides from `~/.config/implicit/themes/{name}.toml`.
/// Config directory respects the `$IMPLICIT_CONFIG_DIR` environment variable.
/// All file reads fail silently — falls back to the "dark" built-in on any error.
enum ImplicitThemeLoader {
    static var configDir: URL {
        if let env = ProcessInfo.processInfo.environment["IMPLICIT_CONFIG_DIR"] {
            return URL(fileURLWithPath: env)
        }
        return FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".config/implicit")
    }

    static func load() -> ImplicitTheme {
        let name = readThemeName()
        var theme = ImplicitTheme.builtin(named: name) ?? ImplicitTheme.builtin(named: "dark")!
        applyFileOverrides(to: &theme, name: name)
        return theme
    }

    static func availableNames() -> [String] {
        var names = ImplicitTheme.builtinNames
        let dir = configDir.appendingPathComponent("themes")
        if let entries = try? FileManager.default.contentsOfDirectory(
            at: dir, includingPropertiesForKeys: nil
        ) {
            for entry in entries where entry.pathExtension == "toml" {
                let stem = entry.deletingPathExtension().lastPathComponent
                if !names.contains(stem) { names.append(stem) }
            }
        }
        return names.sorted()
    }

    // MARK: Private

    private static func readThemeName() -> String {
        let path = configDir.appendingPathComponent("config.toml")
        guard let text = try? String(contentsOf: path, encoding: .utf8) else { return "dark" }
        return parseToml(text)["theme"] ?? "dark"
    }

    private static func applyFileOverrides(to theme: inout ImplicitTheme, name: String) {
        let path = configDir.appendingPathComponent("themes/\(name).toml")
        guard let text = try? String(contentsOf: path, encoding: .utf8) else { return }
        let pairs = parseToml(text)

        func set(_ key: String, _ field: inout NSColor) {
            if let hex = pairs[key], let c = NSColor(hexString: hex) { field = c }
        }

        set("heading1",        &theme.heading1)
        set("heading2",        &theme.heading2)
        set("heading3",        &theme.heading3)
        set("heading4",        &theme.heading4)
        set("heading5",        &theme.heading5)
        set("heading6",        &theme.heading6)
        set("bold",            &theme.bold)
        set("italic",          &theme.italic)
        set("code",            &theme.code)
        set("code_bg",         &theme.codeBg)
        set("code_keyword",    &theme.codeKeyword)
        set("code_string",     &theme.codeString)
        set("code_comment",    &theme.codeComment)
        set("code_number",     &theme.codeNumber)
        set("code_type",       &theme.codeType)
        set("code_punctuation",&theme.codePunctuation)
        set("blockquote",      &theme.blockquote)
        set("link",            &theme.link)
        set("rule",            &theme.rule)
        set("conflict_marker", &theme.conflictMarker)
        set("list_marker",     &theme.listMarker)
        set("ui_chrome",       &theme.uiChrome)
        set("git_added",       &theme.gitAdded)
        set("git_deleted",     &theme.gitDeleted)
        set("git_modified",    &theme.gitModified)
        set("git_untracked",   &theme.gitUntracked)
        set("cursor",          &theme.cursor)
        set("selection",       &theme.selection)
        set("background",      &theme.background)
    }

    /// Minimal TOML parser — extracts flat `key = "value"` pairs.
    /// Handles quoted and unquoted values, skips comments and section headers.
    static func parseToml(_ text: String) -> [String: String] {
        var result: [String: String] = [:]
        for line in text.components(separatedBy: .newlines) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard !trimmed.hasPrefix("#"),
                  !trimmed.hasPrefix("["),
                  !trimmed.isEmpty,
                  let eqIdx = trimmed.firstIndex(of: "=") else { continue }
            let key = String(trimmed[..<eqIdx]).trimmingCharacters(in: .whitespaces)
            var value = String(trimmed[trimmed.index(after: eqIdx)...])
                .trimmingCharacters(in: .whitespaces)
            // Strip inline comment
            if let hashIdx = value.firstIndex(of: "#") {
                value = String(value[..<hashIdx]).trimmingCharacters(in: .whitespaces)
            }
            // Strip surrounding quotes
            value = value.trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))
            guard !key.isEmpty, !value.isEmpty else { continue }
            result[key] = value
        }
        return result
    }
}
