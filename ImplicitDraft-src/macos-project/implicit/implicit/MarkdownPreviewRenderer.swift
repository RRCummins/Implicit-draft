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
            blockquote {
              border-left: 3px solid var(--accent);
              padding: 2px 0 2px 14px;
              color: var(--muted);
            }
            blockquote > :last-child {
              margin-bottom: 0;
            }
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
            img {
              display: block;
              max-width: 100%;
              height: auto;
              border: 1px solid var(--border);
              border-radius: 14px;
              margin: 0 0 1rem;
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
        var listItems: [String] = []
        var currentListTag: String?
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

        func flushList() {
            guard let tagName = currentListTag, !listItems.isEmpty else { return }
            html.append("<\(tagName)>\(listItems.joined())</\(tagName)>")
            listItems.removeAll()
            currentListTag = nil
        }

        func flushCodeBlock() {
            guard !codeLines.isEmpty else { return }
            let escapedCode = escapeHTML(codeLines.joined(separator: "\n"))
            if let language = codeFenceLanguage, !language.isEmpty {
                html.append("""
                <div class="code-block">
                  <div class="code-label">\(escapeHTML(language))</div>
                  <pre><code class="language-\(escapeHTML(language.lowercased()))">\(escapedCode)</code></pre>
                </div>
                """)
            } else {
                html.append("<pre><code>\(escapedCode)</code></pre>")
            }
            codeLines.removeAll()
            codeFenceLanguage = nil
        }

        func flushBlockquote() {
            guard !blockquoteLines.isEmpty else { return }
            let inner = markdownToHTML(blockquoteLines.joined(separator: "\n"))
            html.append("<blockquote>\(inner)</blockquote>")
            blockquoteLines.removeAll()
        }

        var lineIndex = 0
        while lineIndex < lines.count {
            let rawLine = lines[lineIndex]
            let line = rawLine.trimmingCharacters(in: .whitespaces)

            if rawLine.hasPrefix("```") {
                flushParagraph()
                flushList()
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
                flushList()
                flushBlockquote()
                lineIndex += 1
                continue
            }

            if let quote = line.dropPrefixIfPresent("> ") ?? line.dropPrefixIfPresent(">") {
                flushParagraph()
                flushList()
                blockquoteLines.append(quote)
                lineIndex += 1
                continue
            }
            flushBlockquote()

            if let table = parseMarkdownTable(lines: lines, startingAt: lineIndex) {
                flushParagraph()
                flushList()
                html.append(renderTableHTML(table))
                lineIndex = table.nextIndex
                continue
            }

            if line == "---" || line == "***" {
                flushParagraph()
                flushList()
                html.append("<hr>")
                lineIndex += 1
                continue
            }

            if let heading = parseHeading(line) {
                flushParagraph()
                flushList()
                html.append("<h\(heading.level)>\(renderInlineMarkdown(heading.text))</h\(heading.level)>")
                lineIndex += 1
                continue
            }

            if let taskItem = parseTaskListItem(line) {
                flushParagraph()
                if currentListTag != "ul class=\"task-list\"" {
                    flushList()
                    currentListTag = "ul class=\"task-list\""
                }
                let checked = taskItem.completed ? " checked" : ""
                let doneClass = taskItem.completed ? " class=\"task-item done\"" : " class=\"task-item\""
                listItems.append(
                    "<li\(doneClass)><input type=\"checkbox\" disabled\(checked)><span>\(renderInlineMarkdown(taskItem.text))</span></li>"
                )
                lineIndex += 1
                continue
            }

            if let item = line.dropPrefixIfPresent("- ") ?? line.dropPrefixIfPresent("* ") {
                flushParagraph()
                if currentListTag != "ul" {
                    flushList()
                    currentListTag = "ul"
                }
                listItems.append("<li>\(renderInlineMarkdown(item))</li>")
                lineIndex += 1
                continue
            }

            if let item = line.captureOrderedListItem() {
                flushParagraph()
                if currentListTag != "ol" {
                    flushList()
                    currentListTag = "ol"
                }
                listItems.append("<li>\(renderInlineMarkdown(item))</li>")
                lineIndex += 1
                continue
            }

            flushList()
            paragraph.append(line)
            lineIndex += 1
        }

        if inCodeBlock { flushCodeBlock() }
        flushParagraph()
        flushList()
        flushBlockquote()
        return html.joined(separator: "\n")
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

    private static func renderInlineMarkdown(_ text: String) -> String {
        var rendered = escapeHTML(text)
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
