import Cocoa

enum StandaloneMarkdownPreviewRenderer {
    static func renderPreviewHTML(for doc: EditorDocument, isMarkdown: Bool) -> String {
        let title = escapeHTML(doc.title)
        let body = isMarkdown ? markdownToHTML(doc.text) : plainTextToHTML(doc.text)
        return """
        <!doctype html>
        <html>
        <head>
          <meta charset="utf-8">
          <meta name="viewport" content="width=device-width, initial-scale=1">
          <title>\(title)</title>
          <style>
            :root {
              color-scheme: dark;
              --bg: \(cssHex(AppPalette.windowBg));
              --panel: \(cssHex(AppPalette.sidebarBg));
              --text: \(cssHex(AppPalette.textPrimary));
              --muted: \(cssHex(AppPalette.textMuted));
              --border: \(cssHex(AppPalette.border));
              --accent: \(cssHex(AppPalette.accent));
              --code: \(cssHex(AppPalette.tabActiveBg));
            }
            html, body {
              margin: 0;
              background: var(--bg);
              color: var(--text);
              font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
            }
            body { padding: 32px 0 56px; }
            main {
              max-width: 820px;
              margin: 0 auto;
              padding: 0 44px;
              box-sizing: border-box;
            }
            h1, h2, h3, h4, h5, h6 {
              line-height: 1.15;
              margin: 1.25em 0 0.5em;
            }
            h1 { font-size: 2rem; }
            h2 { font-size: 1.55rem; }
            h3 { font-size: 1.25rem; }
            p, li, blockquote {
              font-size: 15px;
              line-height: 1.7;
            }
            p, ul, ol, pre, blockquote { margin: 0 0 1rem; }
            ul, ol { padding-left: 1.4rem; }
            li > ul, li > ol {
              margin-top: 0.45rem;
              margin-bottom: 0.45rem;
            }
            .task-list {
              list-style: none;
              padding-left: 0;
            }
            .task-item {
              display: flex;
              align-items: flex-start;
              gap: 10px;
              margin-bottom: 0.55rem;
            }
            .task-item input {
              margin-top: 0.28rem;
              accent-color: var(--accent);
            }
            .task-item.done span {
              color: var(--muted);
              text-decoration: line-through;
            }
            code {
              font-family: "SF Mono", Menlo, monospace;
              font-size: 0.92em;
              background: color-mix(in srgb, var(--code) 88%, transparent);
              border: 1px solid var(--border);
              border-radius: 6px;
              padding: 0.12rem 0.35rem;
            }
            pre {
              background: var(--code);
              border: 1px solid var(--border);
              border-radius: 12px;
              padding: 16px 18px;
              overflow-x: auto;
            }
            .code-block {
              margin: 0 0 1rem;
              border: 1px solid var(--border);
              border-radius: 12px;
              overflow: hidden;
              background: var(--code);
            }
            .code-block .code-label {
              display: inline-flex;
              align-items: center;
              min-height: 28px;
              padding: 0 12px;
              border-bottom: 1px solid var(--border);
              color: var(--muted);
              font-size: 11px;
              font-weight: 600;
              letter-spacing: 0.04em;
              text-transform: uppercase;
              background: color-mix(in srgb, var(--panel) 48%, transparent);
            }
            .code-block pre {
              margin: 0;
              border: 0;
              border-radius: 0;
            }
            pre code {
              background: transparent;
              border: 0;
              padding: 0;
            }
            .tok-keyword { color: #ff7ab2; }
            .tok-type { color: #78c2ff; }
            .tok-string { color: #a7f3a1; }
            .tok-comment { color: #8b949e; }
            .tok-number { color: #f6c177; }
            .tok-property { color: #8ad4ff; }
            blockquote {
              border-left: 3px solid var(--accent);
              padding: 2px 0 2px 14px;
              color: var(--muted);
              background: color-mix(in srgb, var(--panel) 18%, transparent);
              border-radius: 0 10px 10px 0;
            }
            blockquote > :last-child {
              margin-bottom: 0;
            }
            .callout {
              margin: 0 0 1rem;
              padding: 14px 16px 14px 18px;
              border: 1px solid var(--border);
              border-left: 3px solid var(--accent);
              border-radius: 14px;
              background: color-mix(in srgb, var(--panel) 42%, transparent);
            }
            .callout-title {
              display: flex;
              align-items: center;
              gap: 10px;
              margin-bottom: 10px;
              font-size: 12px;
              font-weight: 700;
              letter-spacing: 0.05em;
              text-transform: uppercase;
            }
            .callout-icon {
              width: 8px;
              height: 8px;
              border-radius: 999px;
              background: currentColor;
            }
            .callout-content > :last-child {
              margin-bottom: 0;
            }
            .callout-note { color: #78c2ff; }
            .callout-tip { color: #78f0b0; }
            .callout-success { color: #78f0b0; }
            .callout-warning { color: #f6c177; }
            .callout-danger { color: #ff8f8f; }
            .callout-important { color: #c4a1ff; }
            .callout-quote { color: var(--muted); }
            hr {
              border: 0;
              height: 1px;
              background: var(--border);
              margin: 1.5rem 0;
            }
            a {
              color: var(--accent);
              text-decoration: none;
            }
            a:hover {
              text-decoration: underline;
            }
            .wikilink {
              font-weight: 600;
            }
            img {
              display: block;
              max-width: 100%;
              max-height: 520px;
              height: auto;
              border: 1px solid var(--border);
              border-radius: 14px;
              margin: 0 0 1rem;
              object-fit: contain;
            }
            .table-wrap {
              margin: 0 0 1rem;
              overflow-x: auto;
              border: 1px solid var(--border);
              border-radius: 12px;
              background: color-mix(in srgb, var(--panel) 35%, transparent);
            }
            table {
              width: 100%;
              border-collapse: separate;
              border-spacing: 0;
              font-size: 14px;
              margin: 0;
            }
            th, td {
              padding: 10px 12px;
              text-align: left;
              vertical-align: top;
              border-right: 1px solid var(--border);
              border-bottom: 1px solid var(--border);
            }
            th:last-child, td:last-child { border-right: 0; }
            tbody tr:last-child td { border-bottom: 0; }
            thead th {
              background: color-mix(in srgb, var(--panel) 78%, transparent);
              font-weight: 600;
            }
            tbody tr:nth-child(even) td {
              background: color-mix(in srgb, var(--panel) 28%, transparent);
            }
          </style>
        </head>
        <body>
          <main>
            \(body)
          </main>
        </body>
        </html>
        """
    }

    private static func plainTextToHTML(_ text: String) -> String {
        "<pre><code>\(escapeHTML(text))</code></pre>"
    }

    private static func markdownToHTML(_ markdown: String) -> String {
        var html: [String] = []
        var paragraph: [String] = []
        var inCodeBlock = false
        var codeLines: [String] = []
        var codeFenceLanguage: String?
        var blockquoteLines: [String] = []
        let lines = markdown.components(separatedBy: .newlines)

        func flushParagraph() {
            guard !paragraph.isEmpty else { return }
            html.append("<p>\(renderInlineMarkdown(paragraph.joined(separator: " ")))</p>")
            paragraph.removeAll()
        }

        func flushCodeBlock() {
            guard !codeLines.isEmpty else { return }
            html.append(renderCodeBlockHTML(codeLines.joined(separator: "\n"), language: codeFenceLanguage))
            codeLines.removeAll()
            codeFenceLanguage = nil
        }

        func flushBlockquote() {
            guard !blockquoteLines.isEmpty else { return }
            html.append(renderBlockquoteHTML(blockquoteLines))
            blockquoteLines.removeAll()
        }

        var lineIndex = 0
        while lineIndex < lines.count {
            let rawLine = lines[lineIndex]
            let line = rawLine.trimmingCharacters(in: .whitespaces)

            if rawLine.hasPrefix("```") {
                flushParagraph()
                flushBlockquote()
                if inCodeBlock { flushCodeBlock() }
                let fenceSuffix = rawLine.dropFirst(3).trimmingCharacters(in: .whitespaces)
                codeFenceLanguage = fenceSuffix.isEmpty ? nil : fenceSuffix
                inCodeBlock.toggle()
                lineIndex += 1
                continue
            }

            if inCodeBlock {
                codeLines.append(rawLine)
                lineIndex += 1
                continue
            }

            if line.isEmpty {
                flushParagraph()
                flushBlockquote()
                lineIndex += 1
                continue
            }

            if let quote = line.dropPrefixIfPresent("> ") ?? line.dropPrefixIfPresent(">") {
                flushParagraph()
                blockquoteLines.append(quote)
                lineIndex += 1
                continue
            }
            flushBlockquote()

            if let table = parseMarkdownTable(lines: lines, startingAt: lineIndex) {
                flushParagraph()
                html.append(renderTableHTML(table))
                lineIndex = table.nextIndex
                continue
            }

            if line == "---" || line == "***" {
                flushParagraph()
                html.append("<hr>")
                lineIndex += 1
                continue
            }

            if let heading = parseHeading(line) {
                flushParagraph()
                html.append("<h\(heading.level)>\(renderInlineMarkdown(heading.text))</h\(heading.level)>")
                lineIndex += 1
                continue
            }

            if parseListItem(rawLine) != nil {
                flushParagraph()
                let list = renderListBlock(lines: lines, startingAt: lineIndex)
                html.append(list.html)
                lineIndex = list.nextIndex
                continue
            }

            paragraph.append(line)
            lineIndex += 1
        }

        if inCodeBlock { flushCodeBlock() }
        flushParagraph()
        flushBlockquote()
        return html.joined(separator: "\n")
    }

    private static func renderListBlock(lines: [String], startingAt startIndex: Int) -> (html: String, nextIndex: Int) {
        guard let firstItem = parseListItem(lines[startIndex]) else {
            return ("", startIndex)
        }

        let baseIndent = firstItem.indent
        let listKind = firstItem.kind
        var itemsHTML: [String] = []
        var lineIndex = startIndex

        while lineIndex < lines.count {
            guard let item = parseListItem(lines[lineIndex]), item.indent == baseIndent, item.kind == listKind else {
                break
            }

            var itemBodyLines = [item.text]
            var nestedBlocks: [String] = []
            lineIndex += 1

            while lineIndex < lines.count {
                let rawLine = lines[lineIndex]
                let trimmed = rawLine.trimmingCharacters(in: .whitespaces)

                if trimmed.isEmpty {
                    break
                }

                if let nextItem = parseListItem(rawLine) {
                    if nextItem.indent < baseIndent || nextItem.indent == baseIndent {
                        break
                    }

                    let nested = renderListBlock(lines: lines, startingAt: lineIndex)
                    nestedBlocks.append(nested.html)
                    lineIndex = nested.nextIndex
                    continue
                }

                let indent = leadingIndentWidth(of: rawLine)
                if indent > baseIndent {
                    itemBodyLines.append(trimmed)
                    lineIndex += 1
                    continue
                }

                break
            }

            let bodyHTML = renderInlineMarkdown(itemBodyLines.joined(separator: " "))
            let nestedHTML = nestedBlocks.joined()
            itemsHTML.append(renderListItemHTML(kind: item.kind, bodyHTML: bodyHTML, nestedHTML: nestedHTML))
        }

        let wrapperOpen: String
        let wrapperClose: String
        switch listKind {
        case .ordered:
            wrapperOpen = "<ol>"
            wrapperClose = "</ol>"
        case .task:
            wrapperOpen = "<ul class=\"task-list\">"
            wrapperClose = "</ul>"
        case .unordered:
            wrapperOpen = "<ul>"
            wrapperClose = "</ul>"
        }

        return ("\(wrapperOpen)\(itemsHTML.joined())\(wrapperClose)", lineIndex)
    }

    private static func renderListItemHTML(kind: ListKind, bodyHTML: String, nestedHTML: String) -> String {
        switch kind {
        case .unordered, .ordered:
            return "<li>\(bodyHTML)\(nestedHTML)</li>"
        case .task(let completed):
            let checked = completed ? " checked" : ""
            let doneClass = completed ? " class=\"task-item done\"" : " class=\"task-item\""
            return "<li\(doneClass)><input type=\"checkbox\" disabled\(checked)><span>\(bodyHTML)</span></li>\(nestedHTML)"
        }
    }

    private static func parseMarkdownTable(lines: [String], startingAt index: Int)
        -> (header: [String], alignments: [String?], rows: [[String]], nextIndex: Int)?
    {
        guard index + 1 < lines.count,
              let header = splitMarkdownTableRow(lines[index]),
              let alignments = parseMarkdownTableSeparator(lines[index + 1]),
              header.count == alignments.count else {
            return nil
        }

        var rows: [[String]] = []
        var nextIndex = index + 2

        while nextIndex < lines.count {
            let rawLine = lines[nextIndex]
            let trimmed = rawLine.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty,
                  let row = splitMarkdownTableRow(rawLine),
                  row.count == header.count,
                  parseMarkdownTableSeparator(rawLine) == nil else {
                break
            }
            rows.append(row)
            nextIndex += 1
        }

        return (header, alignments, rows, nextIndex)
    }

    private static func splitMarkdownTableRow(_ rawLine: String) -> [String]? {
        let trimmed = rawLine.trimmingCharacters(in: .whitespaces)
        guard trimmed.contains("|") else { return nil }

        let core = trimmed.trimmingCharacters(in: CharacterSet(charactersIn: "|"))
        let parts = core.split(separator: "|", omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespaces) }
        return parts.isEmpty ? nil : parts
    }

    private static func parseMarkdownTableSeparator(_ rawLine: String) -> [String?]? {
        guard let parts = splitMarkdownTableRow(rawLine) else { return nil }

        var alignments: [String?] = []
        for part in parts {
            let compact = part.replacingOccurrences(of: " ", with: "")
            guard !compact.isEmpty, compact.allSatisfy({ $0 == "-" || $0 == ":" }) else {
                return nil
            }

            let dashCount = compact.filter { $0 == "-" }.count
            guard dashCount >= 3 else { return nil }

            let leadingColon = compact.first == ":"
            let trailingColon = compact.last == ":"
            if leadingColon && trailingColon {
                alignments.append("center")
            } else if trailingColon {
                alignments.append("right")
            } else if leadingColon {
                alignments.append("left")
            } else {
                alignments.append(nil)
            }
        }

        return alignments
    }

    private static func renderTableHTML(
        _ table: (header: [String], alignments: [String?], rows: [[String]], nextIndex: Int)
    ) -> String {
        func style(for alignment: String?) -> String {
            guard let alignment else { return "" }
            return " style=\"text-align: \(alignment);\""
        }

        let headerHTML = zip(table.header, table.alignments).map { cell, alignment in
            "<th\(style(for: alignment))>\(renderInlineMarkdown(cell))</th>"
        }.joined()

        let bodyHTML = table.rows.map { row in
            let cells = zip(row, table.alignments).map { cell, alignment in
                "<td\(style(for: alignment))>\(renderInlineMarkdown(cell))</td>"
            }.joined()
            return "<tr>\(cells)</tr>"
        }.joined()

        let bodySection = bodyHTML.isEmpty ? "" : "<tbody>\(bodyHTML)</tbody>"
        return """
        <div class="table-wrap">
          <table>
            <thead><tr>\(headerHTML)</tr></thead>
            \(bodySection)
          </table>
        </div>
        """
    }

    private static func parseHeading(_ line: String) -> (level: Int, text: String)? {
        let hashes = line.prefix { $0 == "#" }
        guard (1...6).contains(hashes.count), line.dropFirst(hashes.count).hasPrefix(" ") else {
            return nil
        }
        let text = line.dropFirst(hashes.count).trimmingCharacters(in: .whitespaces)
        return (hashes.count, text)
    }

    private static func parseTaskListItem(_ line: String) -> (completed: Bool, text: String)? {
        let prefixes = ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] "]
        for prefix in prefixes {
            guard line.hasPrefix(prefix) else { continue }
            let completed = prefix.lowercased().contains("[x]")
            return (completed, String(line.dropFirst(prefix.count)))
        }
        return nil
    }

    private static func parseListItem(_ rawLine: String) -> ParsedListItem? {
        let indent = leadingIndentWidth(of: rawLine)
        let line = rawLine.trimmingCharacters(in: .whitespaces)

        if let task = parseTaskListItem(line) {
            return ParsedListItem(indent: indent, kind: .task(completed: task.completed), text: task.text)
        }

        if let item = line.dropPrefixIfPresent("- ")
            ?? line.dropPrefixIfPresent("* ")
            ?? line.dropPrefixIfPresent("+ ") {
            return ParsedListItem(indent: indent, kind: .unordered, text: item)
        }

        if let item = line.captureOrderedListItem() {
            return ParsedListItem(indent: indent, kind: .ordered, text: item)
        }

        return nil
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

    private static func renderCodeBlockHTML(_ code: String, language: String?) -> String {
        let highlighted = highlightCode(code, language: language)
        if let language, !language.isEmpty {
            return """
            <div class="code-block">
              <div class="code-label">\(escapeHTML(language))</div>
              <pre><code class="language-\(escapeHTML(language.lowercased()))">\(highlighted)</code></pre>
            </div>
            """
        }
        return "<pre><code>\(highlighted)</code></pre>"
    }

    private static func highlightCode(_ code: String, language: String?) -> String {
        guard let language = canonicalLanguage(for: language) else {
            return escapeHTML(code)
        }

        return code.components(separatedBy: .newlines)
            .map { highlightCodeLine($0, language: language) }
            .joined(separator: "\n")
    }

    private static func highlightCodeLine(_ line: String, language: String) -> String {
        let commentPrefix = lineCommentPrefix(for: language)
        let keywords = keywordSet(for: language)
        let typeNames = typeSet(for: language)
        var html = ""
        var index = line.startIndex

        func appendToken(_ className: String, _ value: String) {
            html += "<span class=\"\(className)\">\(escapeHTML(value))</span>"
        }

        while index < line.endIndex {
            if let commentPrefix,
               line[index...].hasPrefix(commentPrefix) {
                appendToken("tok-comment", String(line[index...]))
                break
            }

            let char = line[index]

            if char == "\"" || char == "'" {
                let token = consumeQuotedString(in: line, from: index, delimiter: char)
                appendToken("tok-string", token.value)
                index = token.endIndex
                continue
            }

            if char.isNumber {
                let token = consumeNumber(in: line, from: index)
                appendToken("tok-number", token.value)
                index = token.endIndex
                continue
            }

            if char.isLetter || char == "_" {
                let token = consumeIdentifier(in: line, from: index)
                if keywords.contains(token.value) {
                    appendToken("tok-keyword", token.value)
                } else if typeNames.contains(token.value) {
                    appendToken("tok-type", token.value)
                } else if token.endIndex < line.endIndex, line[token.endIndex] == ":" {
                    appendToken("tok-property", token.value)
                } else {
                    html += escapeHTML(token.value)
                }
                index = token.endIndex
                continue
            }

            html += escapeHTML(String(char))
            index = line.index(after: index)
        }

        return html
    }

    private static func consumeQuotedString(in line: String, from start: String.Index, delimiter: Character)
        -> (value: String, endIndex: String.Index)
    {
        var index = line.index(after: start)
        var escaped = false

        while index < line.endIndex {
            let char = line[index]
            if escaped {
                escaped = false
            } else if char == "\\" {
                escaped = true
            } else if char == delimiter {
                return (String(line[start...index]), line.index(after: index))
            }
            index = line.index(after: index)
        }

        return (String(line[start...]), line.endIndex)
    }

    private static func consumeNumber(in line: String, from start: String.Index)
        -> (value: String, endIndex: String.Index)
    {
        var index = start
        while index < line.endIndex, line[index].isNumber || line[index] == "." {
            index = line.index(after: index)
        }
        return (String(line[start..<index]), index)
    }

    private static func consumeIdentifier(in line: String, from start: String.Index)
        -> (value: String, endIndex: String.Index)
    {
        var index = start
        while index < line.endIndex, line[index].isLetter || line[index].isNumber || line[index] == "_" {
            index = line.index(after: index)
        }
        return (String(line[start..<index]), index)
    }

    private static func canonicalLanguage(for language: String?) -> String? {
        guard let language else { return nil }
        switch language.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "swift": return "swift"
        case "rust", "rs": return "rust"
        case "js", "jsx", "javascript", "ts", "tsx", "typescript": return "javascript"
        case "py", "python": return "python"
        case "sh", "bash", "zsh", "shell": return "shell"
        case "json", "jsonc": return "json"
        default: return nil
        }
    }

    private static func lineCommentPrefix(for language: String) -> String? {
        switch language {
        case "swift", "rust", "javascript":
            return "//"
        case "python", "shell":
            return "#"
        default:
            return nil
        }
    }

    private static func keywordSet(for language: String) -> Set<String> {
        switch language {
        case "swift":
            return ["let", "var", "func", "struct", "class", "enum", "protocol", "extension", "if", "else", "guard", "return", "import", "private", "fileprivate", "internal", "public", "open", "static"]
        case "rust":
            return ["fn", "let", "mut", "struct", "enum", "impl", "trait", "match", "if", "else", "return", "pub", "use", "mod", "const", "static"]
        case "javascript":
            return ["const", "let", "var", "function", "class", "return", "if", "else", "import", "export", "from", "async", "await", "new"]
        case "python":
            return ["def", "class", "return", "if", "elif", "else", "import", "from", "as", "for", "while", "try", "except", "with", "lambda"]
        case "shell":
            return ["if", "then", "else", "fi", "for", "do", "done", "case", "esac", "function", "in"]
        case "json":
            return ["true", "false", "null"]
        default:
            return []
        }
    }

    private static func typeSet(for language: String) -> Set<String> {
        switch language {
        case "swift":
            return ["String", "Int", "Bool", "Double", "URL", "Date", "Data", "UUID"]
        case "rust":
            return ["String", "Vec", "Option", "Result", "Self"]
        case "javascript":
            return ["Promise", "Object", "Array", "Map", "Set"]
        case "python":
            return ["str", "int", "bool", "list", "dict", "set", "tuple"]
        default:
            return []
        }
    }

    private static func renderInlineMarkdown(_ text: String) -> String {
        var rendered = escapeHTML(text)
        rendered = rendered.replacingOccurrences(
            of: #"\[\[([^\]|]+)\|([^\]]+)\]\]"#,
            with: "<a href=\"#\" class=\"wikilink\" data-target=\"$1\">$2</a>",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"\[\[([^\]]+)\]\]"#,
            with: "<a href=\"#\" class=\"wikilink\" data-target=\"$1\">$1</a>",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"!\[([^\]]*)\]\(([^)]+)\)"#,
            with: "<img src=\"$2\" alt=\"$1\">",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"`([^`]+)`"#,
            with: "<code>$1</code>",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"\*\*([^*]+)\*\*"#,
            with: "<strong>$1</strong>",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"(?<!\*)\*([^*]+)\*(?!\*)"#,
            with: "<em>$1</em>",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"~~([^~]+)~~"#,
            with: "<del>$1</del>",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"\[([^\]]+)\]\(([^)]+)\)"#,
            with: "<a href=\"$2\">$1</a>",
            options: .regularExpression
        )
        return rendered
    }

    private static func escapeHTML(_ value: String) -> String {
        value
            .replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
            .replacingOccurrences(of: "\"", with: "&quot;")
    }

    private static func cssHex(_ color: NSColor) -> String {
        let rgb = color.usingColorSpace(.deviceRGB) ?? color
        let r = Int((rgb.redComponent * 255).rounded())
        let g = Int((rgb.greenComponent * 255).rounded())
        let b = Int((rgb.blueComponent * 255).rounded())
        return String(format: "#%02X%02X%02X", r, g, b)
    }

    private static func renderBlockquoteHTML(_ lines: [String]) -> String {
        guard let firstLine = lines.first else { return "" }

        if let callout = parseCallout(firstLine) {
            let title = callout.title.isEmpty ? callout.kind.capitalized : callout.title
            var bodyLines = Array(lines.dropFirst())
            if !callout.inlineBody.isEmpty {
                bodyLines.insert(callout.inlineBody, at: 0)
            }
            let bodyHTML = bodyLines.isEmpty ? "" : markdownToHTML(bodyLines.joined(separator: "\n"))
            let kindClass = escapeHTML(callout.kind)
            return """
            <div class="callout callout-\(kindClass)">
              <div class="callout-title"><span class="callout-icon"></span><span>\(escapeHTML(title))</span></div>
              <div class="callout-content">\(bodyHTML)</div>
            </div>
            """
        }

        let inner = markdownToHTML(lines.joined(separator: "\n"))
        return "<blockquote>\(inner)</blockquote>"
    }

    private static func parseCallout(_ line: String) -> (kind: String, title: String, inlineBody: String)? {
        let pattern = #"^\[!([A-Za-z]+)\]([+-])?\s*(.*)$"#
        guard let regex = try? NSRegularExpression(pattern: pattern),
              let match = regex.firstMatch(in: line, range: NSRange(line.startIndex..., in: line)),
              let kindRange = Range(match.range(at: 1), in: line),
              let bodyRange = Range(match.range(at: 3), in: line) else {
            return nil
        }

        let kind = line[kindRange].lowercased()
        let remainder = String(line[bodyRange]).trimmingCharacters(in: .whitespaces)
        return (kind: kind, title: remainder, inlineBody: "")
    }
}

private struct ParsedListItem {
    let indent: Int
    let kind: ListKind
    let text: String
}

private enum ListKind: Equatable {
    case unordered
    case ordered
    case task(completed: Bool)
}

private extension String {
    func dropPrefixIfPresent(_ prefix: String) -> String? {
        hasPrefix(prefix) ? String(dropFirst(prefix.count)) : nil
    }

    func captureOrderedListItem() -> String? {
        guard let dot = firstIndex(of: ".") else { return nil }
        let prefix = self[..<dot]
        guard !prefix.isEmpty, prefix.allSatisfy(\.isNumber) else { return nil }
        let remainderStart = index(after: dot)
        guard remainderStart < endIndex, self[remainderStart] == " " else { return nil }
        let contentStart = index(after: remainderStart)
        guard contentStart <= endIndex else { return nil }
        return String(self[contentStart...])
    }
}
