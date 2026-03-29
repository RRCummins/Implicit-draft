// DesignSystem.swift
// App-shell palette and layout constants for Implicit Standalone.

import Cocoa

// MARK: - NSColor convenience

extension NSColor {
    /// Creates an sRGB color from a 24-bit hex literal, e.g. `NSColor(rgb: 0x58a6ff)`.
    convenience init(rgb hex: UInt32) {
        let r = CGFloat((hex >> 16) & 0xff) / 255
        let g = CGFloat((hex >> 8)  & 0xff) / 255
        let b = CGFloat(hex         & 0xff) / 255
        self.init(srgbRed: r, green: g, blue: b, alpha: 1)
    }

    /// Parses `"#rrggbb"` or `"rrggbb"` strings. Returns nil on invalid input.
    convenience init?(hexString: String) {
        let stripped = hexString.trimmingCharacters(in: .whitespaces)
        let hex = stripped.hasPrefix("#") ? String(stripped.dropFirst()) : stripped
        guard hex.count == 6, let value = UInt32(hex, radix: 16) else { return nil }
        self.init(rgb: value)
    }
}

// MARK: - App chrome palette

/// Static UI chrome colors — independent of the editor theme.
/// These define the structural look of the app shell: tab bar, sidebar, status bar.
/// Editor content colors come from `ImplicitTheme` instead.
enum AppPalette {
    static let tabBarBg    = NSColor(rgb: 0x010409)  // near black — tab bar background
    static let tabActiveBg = NSColor(rgb: 0x0d1117)  // matches windowBg — seamless active tab
    static let windowBg    = NSColor(rgb: 0x0d1117)  // main editor background
    static let sidebarBg   = NSColor(rgb: 0x161b22)  // sidebar + status bar background
    static let border      = NSColor(rgb: 0x30363d)  // dividers and edges
    static let textPrimary = NSColor(rgb: 0xe6edf3)  // primary text
    static let textMuted   = NSColor(rgb: 0x8b949e)  // secondary labels
    static let accent      = NSColor(rgb: 0x58a6ff)  // active state, focus ring, cursor
}

// MARK: - Layout metrics

/// Static layout constants. All values are in points (logical, not physical pixels).
enum AppMetrics {
    static let tabBarHeight:       CGFloat = 36
    static let sidebarWidth:       CGFloat = 220
    static let statusBarHeight:    CGFloat = 24
    /// Left inset for tab bar content to clear macOS traffic light buttons.
    static let tabBarLeadInset:    CGFloat = 80
    static let sidebarRowHeight:   CGFloat = 32
    static let bodyFontSize:       CGFloat = 13
    static let monoFontSize:       CGFloat = 13
    static let editorInsetH:       CGFloat = 24
    static let editorInsetV:       CGFloat = 20
    static let lineHeightMultiple: CGFloat = 1.5
    static let tabMinWidth:        CGFloat = 100
}
