import Cocoa

@MainActor
enum MarkdownEditorStyler {
    enum PresentationMode {
        case source
        case live(revealedRange: NSRange?)
    }

    static func apply(
        to textView: NSTextView,
        text: String,
        isMarkdown: Bool,
        mode: PresentationMode
    ) {
        guard let textStorage = textView.textStorage else { return }

        let fullRange = NSRange(location: 0, length: (text as NSString).length)
        let selectedRange = textView.selectedRange()
        let scrollPoint = (textView.enclosingScrollView?.contentView.bounds.origin) ?? .zero

        let undoManager = textView.undoManager
        undoManager?.disableUndoRegistration()
        defer { undoManager?.enableUndoRegistration() }

        isMarkdown ? styleMarkdown(textStorage: textStorage, text: text, fullRange: fullRange, mode: mode)
                   : stylePlainText(textStorage: textStorage, text: text, fullRange: fullRange)

        textView.setSelectedRange(selectedRange)
        if let scrollView = textView.enclosingScrollView {
            scrollView.contentView.scroll(to: scrollPoint)
            scrollView.reflectScrolledClipView(scrollView.contentView)
        }
    }

    private static func stylePlainText(textStorage: NSTextStorage, text: String, fullRange: NSRange) {
        textStorage.beginEditing()
        defer { textStorage.endEditing() }
        textStorage.setAttributes(baseCodeAttributes(), range: fullRange)
    }

    private static func styleMarkdown(
        textStorage: NSTextStorage,
        text: String,
        fullRange: NSRange,
        mode: PresentationMode
    ) {
        let nsText = text as NSString
        let bodyFont = NSFont.systemFont(ofSize: 15, weight: .regular)
        let monoFont = NSFont.monospacedSystemFont(ofSize: AppMetrics.monoFontSize, weight: .regular)

        textStorage.beginEditing()
        defer { textStorage.endEditing() }

        textStorage.setAttributes([
            .font: bodyFont,
            .foregroundColor: AppPalette.textPrimary,
            .backgroundColor: AppPalette.windowBg,
            .paragraphStyle: bodyParagraphStyle()
        ], range: fullRange)

        var lineNumber = 0
        var codeFenceLanguage: String?
        nsText.enumerateSubstrings(in: NSRange(location: 0, length: nsText.length), options: .byLines) {
            substring, substringRange, _, _ in
            guard let line = substring else { return }
            defer { lineNumber += 1 }

            let lineRange = nsText.lineRange(for: substringRange)
            let lineContentRange = NSRange(location: substringRange.location, length: substringRange.length)
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            if line.hasPrefix("```") {
                let lang = String(line.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                let title = lang.isEmpty ? "code fence" : lang.uppercased()
                applyWholeLine(
                    textStorage,
                    range: lineRange,
                    font: NSFont.monospacedSystemFont(ofSize: 12, weight: .semibold),
                    color: AppPalette.textMuted,
                    background: AppPalette.sidebarBg.withAlphaComponent(0.45),
                    paragraphStyle: codeParagraphStyle()
                )
                let titleRange = (line as NSString).range(of: title)
                if titleRange.location != NSNotFound {
                    let shiftedRange = NSRange(location: lineContentRange.location + titleRange.location, length: titleRange.length)
                    textStorage.addAttribute(.foregroundColor, value: AppPalette.accent.withAlphaComponent(0.9), range: shiftedRange)
                }
                codeFenceLanguage = codeFenceLanguage == nil ? lang : nil
                return
            }

            if codeFenceLanguage != nil {
                applyWholeLine(
                    textStorage,
                    range: lineRange,
                    font: monoFont,
                    color: AppPalette.textPrimary,
                    background: AppPalette.sidebarBg.withAlphaComponent(0.28),
                    paragraphStyle: codeParagraphStyle()
                )
                styleInlineCodeTokens(in: line, nsText: nsText, lineContentRange: lineContentRange, textStorage: textStorage)
                return
            }

            if let heading = parseHeading(trimmed) {
                applyHeading(
                    line: line,
                    nsText: nsText,
                    lineRange: lineRange,
                    lineContentRange: lineContentRange,
                    level: heading.level,
                    bodyFont: bodyFont,
                    textStorage: textStorage,
                    mode: mode
                )
                return
            }

            if isThematicBreak(trimmed) {
                applyWholeLine(
                    textStorage,
                    range: lineRange,
                    font: NSFont.monospacedSystemFont(ofSize: 12, weight: .regular),
                    color: AppPalette.border,
                    background: AppPalette.windowBg,
                    paragraphStyle: bodyParagraphStyle()
                )
                return
            }

            if trimmed.hasPrefix(">") {
                applyBlockquoteLine(line: line, nsText: nsText, lineRange: lineRange, lineContentRange: lineContentRange, textStorage: textStorage, mode: mode)
                return
            }

            if parseTableRow(trimmed) != nil {
                applyWholeLine(
                    textStorage,
                    range: lineRange,
                    font: monoFont,
                    color: AppPalette.textPrimary,
                    background: AppPalette.sidebarBg.withAlphaComponent(0.16),
                    paragraphStyle: tableParagraphStyle()
                )
                styleTableSyntax(in: line, nsText: nsText, lineContentRange: lineContentRange, textStorage: textStorage, mode: mode)
                styleInlineSyntax(in: line, nsText: nsText, lineContentRange: lineContentRange, textStorage: textStorage, mode: mode)
                return
            }

            if let task = parseTaskListItem(trimmed) {
                applyListLine(
                    line: line,
                    nsText: nsText,
                    lineRange: lineRange,
                    lineContentRange: lineContentRange,
                    markerLength: task.prefixLength,
                    accentColor: task.completed ? AppPalette.textMuted : AppPalette.accent,
                    textStorage: textStorage,
                    mode: mode
                )
                styleInlineSyntax(in: line, nsText: nsText, lineContentRange: lineContentRange, textStorage: textStorage, mode: mode)
                return
            }

            if let marker = parseListMarker(trimmed) {
                applyListLine(
                    line: line,
                    nsText: nsText,
                    lineRange: lineRange,
                    lineContentRange: lineContentRange,
                    markerLength: marker,
                    accentColor: AppPalette.accent.withAlphaComponent(0.8),
                    textStorage: textStorage,
                    mode: mode
                )
                styleInlineSyntax(in: line, nsText: nsText, lineContentRange: lineContentRange, textStorage: textStorage, mode: mode)
                return
            }

            applyWholeLine(
                textStorage,
                range: lineRange,
                font: bodyFont,
                color: AppPalette.textPrimary,
                background: AppPalette.windowBg,
                paragraphStyle: bodyParagraphStyle()
            )
            styleInlineSyntax(in: line, nsText: nsText, lineContentRange: lineContentRange, textStorage: textStorage, mode: mode)
        }
    }

    private static func applyHeading(
        line: String,
        nsText: NSString,
        lineRange: NSRange,
        lineContentRange: NSRange,
        level: Int,
        bodyFont: NSFont,
        textStorage: NSTextStorage,
        mode: PresentationMode
    ) {
        let fontSize: CGFloat
        switch level {
        case 1: fontSize = 30
        case 2: fontSize = 24
        case 3: fontSize = 20
        case 4: fontSize = 17
        case 5: fontSize = 15
        default: fontSize = 14
        }

        applyWholeLine(
            textStorage,
            range: lineRange,
            font: NSFont.systemFont(ofSize: fontSize, weight: .semibold),
            color: AppPalette.textPrimary,
            background: AppPalette.windowBg,
            paragraphStyle: headingParagraphStyle(level: level)
        )

        let markerLength = min(level + 1, lineContentRange.length)
        if markerLength > 0 {
            let markerRange = NSRange(location: lineContentRange.location, length: markerLength)
            textStorage.addAttributes([
                .foregroundColor: delimiterColor(for: markerRange, mode: mode),
                .font: NSFont.monospacedSystemFont(ofSize: max(12, bodyFont.pointSize - 1), weight: .regular)
            ], range: markerRange)
        }

        styleInlineSyntax(in: line, nsText: nsText, lineContentRange: lineContentRange, textStorage: textStorage, mode: mode)
    }

    private static func applyBlockquoteLine(
        line: String,
        nsText: NSString,
        lineRange: NSRange,
        lineContentRange: NSRange,
        textStorage: NSTextStorage,
        mode: PresentationMode
    ) {
        applyWholeLine(
            textStorage,
            range: lineRange,
            font: NSFont.systemFont(ofSize: 15, weight: .regular),
            color: AppPalette.textPrimary.withAlphaComponent(0.95),
            background: AppPalette.sidebarBg.withAlphaComponent(0.16),
            paragraphStyle: blockquoteParagraphStyle()
        )

        let markerLength = line.hasPrefix("> ") ? 2 : 1
        let markerRange = NSRange(location: lineContentRange.location, length: min(markerLength, lineContentRange.length))
        textStorage.addAttributes([
            .foregroundColor: markerAccentColor(for: markerRange, accentColor: AppPalette.accent, mode: mode),
            .font: NSFont.monospacedSystemFont(ofSize: 12, weight: .semibold)
        ], range: markerRange)

        styleInlineSyntax(in: line, nsText: nsText, lineContentRange: lineContentRange, textStorage: textStorage, mode: mode)
    }

    private static func applyListLine(
        line: String,
        nsText: NSString,
        lineRange: NSRange,
        lineContentRange: NSRange,
        markerLength: Int,
        accentColor: NSColor,
        textStorage: NSTextStorage,
        mode: PresentationMode
    ) {
        let indent = leadingIndentWidth(of: line)
        applyWholeLine(
            textStorage,
            range: lineRange,
            font: NSFont.systemFont(ofSize: 15, weight: .regular),
            color: AppPalette.textPrimary,
            background: AppPalette.windowBg,
            paragraphStyle: listParagraphStyle(indent: indent)
        )

        let markerRange = NSRange(location: lineContentRange.location + indent, length: min(markerLength, max(0, lineContentRange.length - indent)))
        textStorage.addAttributes([
            .foregroundColor: markerAccentColor(for: markerRange, accentColor: accentColor, mode: mode),
            .font: NSFont.monospacedSystemFont(ofSize: 12, weight: .medium)
        ], range: markerRange)
    }

    private static func styleInlineSyntax(
        in line: String,
        nsText: NSString,
        lineContentRange: NSRange,
        textStorage: NSTextStorage,
        mode: PresentationMode
    ) {
        styleRegex(#"`([^`]+)`"#, in: line, lineContentRange: lineContentRange, textStorage: textStorage) { match, lineRange, storage in
            let codeRange = match.range(at: 1)
            let full = match.range(at: 0)
            storage.addAttributes([
                .foregroundColor: delimiterColor(for: shifted(full, by: lineRange.location), mode: mode),
                .font: NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
            ], range: shifted(full, by: lineRange.location))
            storage.addAttributes([
                .foregroundColor: AppPalette.textPrimary,
                .backgroundColor: AppPalette.sidebarBg.withAlphaComponent(0.28),
                .font: NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
            ], range: shifted(codeRange, by: lineRange.location))
        }

        styleRegex(#"\*\*([^*]+)\*\*"#, in: line, lineContentRange: lineContentRange, textStorage: textStorage) { match, lineRange, storage in
            applyDelimitedRange(match: match, contentGroup: 1, lineRange: lineRange, contentAttributes: [
                .font: NSFont.systemFont(ofSize: 15, weight: .bold),
                .foregroundColor: AppPalette.textPrimary
            ], delimiterAttributes: [
                .foregroundColor: AppPalette.textMuted
            ], storage: storage, mode: mode)
        }

        styleRegex(#"(?<!\*)\*([^*]+)\*(?!\*)"#, in: line, lineContentRange: lineContentRange, textStorage: textStorage) { match, lineRange, storage in
            applyDelimitedRange(match: match, contentGroup: 1, lineRange: lineRange, contentAttributes: [
                .font: NSFontManager.shared.convert(.systemFont(ofSize: 15, weight: .regular), toHaveTrait: .italicFontMask),
                .foregroundColor: AppPalette.textPrimary
            ], delimiterAttributes: [
                .foregroundColor: AppPalette.textMuted
            ], storage: storage, mode: mode)
        }

        styleRegex(#"~~([^~]+)~~"#, in: line, lineContentRange: lineContentRange, textStorage: textStorage) { match, lineRange, storage in
            applyDelimitedRange(match: match, contentGroup: 1, lineRange: lineRange, contentAttributes: [
                .foregroundColor: AppPalette.textMuted,
                .strikethroughStyle: NSUnderlineStyle.single.rawValue
            ], delimiterAttributes: [
                .foregroundColor: AppPalette.textMuted.withAlphaComponent(0.75)
            ], storage: storage, mode: mode)
        }

        styleRegex(#"\[([^\]]+)\]\(([^)]+)\)"#, in: line, lineContentRange: lineContentRange, textStorage: textStorage) { match, lineRange, storage in
            let fullRange = shifted(match.range(at: 0), by: lineRange.location)
            let showSyntax = isRangeRevealed(fullRange, mode: mode)
            if let textRange = rangeForGroup(match, 1, lineRange), let urlRange = rangeForGroup(match, 2, lineRange) {
                storage.addAttributes([
                    .foregroundColor: AppPalette.accent,
                    .underlineStyle: NSUnderlineStyle.single.rawValue
                ], range: textRange)
                storage.addAttributes([
                    .foregroundColor: showSyntax ? AppPalette.textMuted : hiddenDelimiterColor(),
                    .font: NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
                ], range: urlRange)
            }
            styleMarkdownDelimiters(for: match, lineRange: lineRange, storage: storage, mode: mode)
        }

        styleRegex(#"\[\[([^\]|]+)\|([^\]]+)\]\]"#, in: line, lineContentRange: lineContentRange, textStorage: textStorage) { match, lineRange, storage in
            let fullRange = shifted(match.range(at: 0), by: lineRange.location)
            let showSyntax = isRangeRevealed(fullRange, mode: mode)
            if let targetRange = rangeForGroup(match, 1, lineRange), let aliasRange = rangeForGroup(match, 2, lineRange) {
                storage.addAttributes([.foregroundColor: showSyntax ? AppPalette.textMuted : hiddenDelimiterColor()], range: targetRange)
                storage.addAttributes([
                    .foregroundColor: AppPalette.accent,
                    .font: NSFont.systemFont(ofSize: 15, weight: .semibold)
                ], range: aliasRange)
            }
            styleMarkdownDelimiters(for: match, lineRange: lineRange, storage: storage, mode: mode)
        }

        styleRegex(#"\[\[([^\]]+)\]\]"#, in: line, lineContentRange: lineContentRange, textStorage: textStorage) { match, lineRange, storage in
            if let targetRange = rangeForGroup(match, 1, lineRange) {
                storage.addAttributes([
                    .foregroundColor: AppPalette.accent,
                    .font: NSFont.systemFont(ofSize: 15, weight: .semibold)
                ], range: targetRange)
            }
            styleMarkdownDelimiters(for: match, lineRange: lineRange, storage: storage, mode: mode)
        }
    }

    private static func styleTableSyntax(
        in line: String,
        nsText: NSString,
        lineContentRange: NSRange,
        textStorage: NSTextStorage,
        mode: PresentationMode
    ) {
        let nsLine = line as NSString
        let fullLength = nsLine.length
        for idx in 0..<fullLength where nsLine.character(at: idx) == 124 {
            let range = NSRange(location: lineContentRange.location + idx, length: 1)
            textStorage.addAttributes([
                .foregroundColor: delimiterColor(for: range, mode: mode)
            ], range: range)
        }
    }

    private static func styleInlineCodeTokens(
        in line: String,
        nsText: NSString,
        lineContentRange: NSRange,
        textStorage: NSTextStorage
    ) {
        styleRegex(#"//.*$"#, in: line, lineContentRange: lineContentRange, options: [.anchorsMatchLines], textStorage: textStorage) { match, lineRange, storage in
            storage.addAttributes([
                .foregroundColor: AppPalette.textMuted
            ], range: shifted(match.range(at: 0), by: lineRange.location))
        }

        styleRegex(#"\"([^\"\\]|\\.)*\""#, in: line, lineContentRange: lineContentRange, textStorage: textStorage) { match, lineRange, storage in
            storage.addAttributes([
                .foregroundColor: NSColor(calibratedRed: 0.65, green: 0.95, blue: 0.63, alpha: 1)
            ], range: shifted(match.range(at: 0), by: lineRange.location))
        }

        styleRegex(#"\b(let|var|func|struct|class|enum|protocol|extension|if|else|guard|return|import|private|fileprivate|internal|public|open|static)\b"#, in: line, lineContentRange: lineContentRange, textStorage: textStorage) { match, lineRange, storage in
            storage.addAttributes([
                .foregroundColor: NSColor(calibratedRed: 1.0, green: 0.48, blue: 0.70, alpha: 1),
                .font: NSFont.monospacedSystemFont(ofSize: 13, weight: .semibold)
            ], range: shifted(match.range(at: 0), by: lineRange.location))
        }
    }

    private static func styleRegex(
        _ pattern: String,
        in line: String,
        lineContentRange: NSRange,
        textStorage: NSTextStorage,
        handler: (NSTextCheckingResult, NSRange, NSTextStorage) -> Void
    ) {
        styleRegex(pattern, in: line, lineContentRange: lineContentRange, options: [], textStorage: textStorage, handler: handler)
    }

    private static func styleRegex(
        _ pattern: String,
        in line: String,
        lineContentRange: NSRange,
        options: NSRegularExpression.Options,
        textStorage: NSTextStorage,
        handler: (NSTextCheckingResult, NSRange, NSTextStorage) -> Void
    ) {
        guard let regex = try? NSRegularExpression(pattern: pattern, options: options) else { return }
        let lineNSRange = NSRange(location: 0, length: (line as NSString).length)
        regex.enumerateMatches(in: line, range: lineNSRange) { match, _, _ in
            guard let match else { return }
            handler(match, lineContentRange, textStorage)
        }
    }

    private static func applyDelimitedRange(
        match: NSTextCheckingResult,
        contentGroup: Int,
        lineRange: NSRange,
        contentAttributes: [NSAttributedString.Key: Any],
        delimiterAttributes: [NSAttributedString.Key: Any],
        storage: NSTextStorage,
        mode: PresentationMode
    ) {
        let full = shifted(match.range(at: 0), by: lineRange.location)
        let content = shifted(match.range(at: contentGroup), by: lineRange.location)
        var adjustedDelimiterAttributes = delimiterAttributes
        adjustedDelimiterAttributes[.foregroundColor] = delimiterColor(for: full, mode: mode)
        storage.addAttributes(adjustedDelimiterAttributes, range: full)
        storage.addAttributes(contentAttributes, range: content)
    }

    private static func styleMarkdownDelimiters(
        for match: NSTextCheckingResult,
        lineRange: NSRange,
        storage: NSTextStorage,
        mode: PresentationMode
    ) {
        let full = shifted(match.range(at: 0), by: lineRange.location)
        storage.addAttributes([
            .foregroundColor: delimiterColor(for: full, mode: mode)
        ], range: full)
    }

    private static func isRangeRevealed(_ range: NSRange, mode: PresentationMode) -> Bool {
        switch mode {
        case .source:
            return true
        case .live(let revealedRange):
            guard let revealedRange else { return false }
            return NSIntersectionRange(range, revealedRange).length > 0
        }
    }

    private static func delimiterColor(for range: NSRange, mode: PresentationMode) -> NSColor {
        isRangeRevealed(range, mode: mode) ? AppPalette.textMuted : hiddenDelimiterColor()
    }

    private static func markerAccentColor(for range: NSRange, accentColor: NSColor, mode: PresentationMode) -> NSColor {
        isRangeRevealed(range, mode: mode)
            ? accentColor
            : accentColor.withAlphaComponent(0.22)
    }

    private static func hiddenDelimiterColor() -> NSColor {
        AppPalette.textPrimary.withAlphaComponent(0.07)
    }

    private static func rangeForGroup(_ match: NSTextCheckingResult, _ group: Int, _ lineRange: NSRange) -> NSRange? {
        let raw = match.range(at: group)
        guard raw.location != NSNotFound else { return nil }
        return shifted(raw, by: lineRange.location)
    }

    private static func shifted(_ range: NSRange, by delta: Int) -> NSRange {
        NSRange(location: range.location + delta, length: range.length)
    }

    private static func applyWholeLine(
        _ textStorage: NSTextStorage,
        range: NSRange,
        font: NSFont,
        color: NSColor,
        background: NSColor,
        paragraphStyle: NSParagraphStyle
    ) {
        textStorage.addAttributes([
            .font: font,
            .foregroundColor: color,
            .backgroundColor: background,
            .paragraphStyle: paragraphStyle
        ], range: range)
    }

    private static func baseCodeAttributes() -> [NSAttributedString.Key: Any] {
        [
            .font: NSFont.monospacedSystemFont(ofSize: AppMetrics.monoFontSize, weight: .regular),
            .foregroundColor: AppPalette.textPrimary,
            .backgroundColor: AppPalette.windowBg,
            .paragraphStyle: bodyParagraphStyle()
        ]
    }

    private static func bodyParagraphStyle() -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = 4
        style.paragraphSpacing = 8
        return style
    }

    private static func headingParagraphStyle(level: Int) -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = level <= 2 ? 6 : 4
        style.paragraphSpacingBefore = level == 1 ? 18 : 14
        style.paragraphSpacing = 8
        return style
    }

    private static func blockquoteParagraphStyle() -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.headIndent = 18
        style.firstLineHeadIndent = 18
        style.paragraphSpacing = 6
        style.lineSpacing = 4
        return style
    }

    private static func listParagraphStyle(indent: Int) -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        let base = CGFloat(18 + indent)
        style.headIndent = base
        style.firstLineHeadIndent = CGFloat(indent)
        style.paragraphSpacing = 4
        style.lineSpacing = 3
        return style
    }

    private static func codeParagraphStyle() -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = 2
        style.paragraphSpacing = 0
        style.headIndent = 18
        style.firstLineHeadIndent = 18
        return style
    }

    private static func tableParagraphStyle() -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = 2
        style.paragraphSpacing = 2
        return style
    }

    private static func parseHeading(_ line: String) -> (level: Int, text: String)? {
        let hashes = line.prefix { $0 == "#" }
        guard (1...6).contains(hashes.count), line.dropFirst(hashes.count).hasPrefix(" ") else {
            return nil
        }
        return (hashes.count, String(line.dropFirst(hashes.count).trimmingCharacters(in: .whitespaces)))
    }

    private static func isThematicBreak(_ line: String) -> Bool {
        line == "---" || line == "***"
    }

    private static func parseTaskListItem(_ line: String) -> (completed: Bool, prefixLength: Int)? {
        let prefixes = ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] ", "+ [ ] ", "+ [x] ", "+ [X] "]
        for prefix in prefixes where line.hasPrefix(prefix) {
            return (prefix.lowercased().contains("[x]"), prefix.count)
        }
        return nil
    }

    private static func parseListMarker(_ line: String) -> Int? {
        if line.hasPrefix("- ") || line.hasPrefix("* ") || line.hasPrefix("+ ") {
            return 2
        }
        let nsLine = line as NSString
        let pattern = #"^\d+\.\s"#
        guard let regex = try? NSRegularExpression(pattern: pattern),
              let match = regex.firstMatch(in: line, range: NSRange(location: 0, length: nsLine.length)) else {
            return nil
        }
        return match.range.length
    }

    private static func parseTableRow(_ line: String) -> Bool? {
        line.contains("|") ? true : nil
    }

    private static func leadingIndentWidth(of rawLine: String) -> Int {
        var width = 0
        for char in rawLine {
            switch char {
            case " ":
                width += 1
            case "\t":
                width += 4
            default:
                return width
            }
        }
        return width
    }
}
