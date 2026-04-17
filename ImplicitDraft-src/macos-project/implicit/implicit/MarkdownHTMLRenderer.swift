import Cocoa

@MainActor
enum MarkdownHTMLRenderer {
    static func renderBody(from blocks: [MarkdownBlock]) -> String {
        blocks.map(renderBlock).joined(separator: "\n")
    }

    private static func renderBlock(_ block: MarkdownBlock) -> String {
        switch block {
        case .paragraph(let text):
            return "<p>\(renderInlineMarkdown(text))</p>"
        case .heading(let level, let text):
            return "<h\(level)>\(renderInlineMarkdown(text))</h\(level)>"
        case .thematicBreak:
            return "<hr>"
        case .blockquote(let blocks):
            return "<blockquote>\(renderBody(from: blocks))</blockquote>"
        case .callout(let kind, let title, let blocks):
            let resolvedTitle = title.isEmpty ? kind.capitalized : title
            return """
            <div class="callout callout-\(escapeHTML(kind))">
              <div class="callout-title"><span class="callout-icon"></span><span>\(escapeHTML(resolvedTitle))</span></div>
              <div class="callout-content">\(renderBody(from: blocks))</div>
            </div>
            """
        case .list(let list):
            return renderList(list)
        case .codeFence(let language, let code):
            return renderCodeBlockHTML(code, language: language)
        case .mermaid(let code):
            return renderMermaidBlockHTML(code)
        case .table(let table):
            return renderTableHTML(table)
        }
    }

    private static func renderList(_ list: MarkdownList) -> String {
        let openTag: String
        let closeTag: String
        switch list.kind {
        case .unordered:
            openTag = "<ul>"
            closeTag = "</ul>"
        case .ordered:
            openTag = "<ol>"
            closeTag = "</ol>"
        case .task:
            openTag = "<ul class=\"task-list\">"
            closeTag = "</ul>"
        }

        let itemsHTML = list.items.map { item in
            let bodyHTML = renderInlineMarkdown(item.text)
            let childHTML = item.childBlocks.isEmpty ? "" : renderBody(from: item.childBlocks)
            if list.kind == .task {
                let checked = item.isChecked == true ? " checked" : ""
                let doneClass = item.isChecked == true ? " class=\"task-item done\"" : " class=\"task-item\""
                return "<li\(doneClass)><input type=\"checkbox\" disabled\(checked)><span>\(bodyHTML)</span>\(childHTML)</li>"
            }
            return "<li>\(bodyHTML)\(childHTML)</li>"
        }.joined()

        return "\(openTag)\(itemsHTML)\(closeTag)"
    }

    private static func renderTableHTML(_ table: MarkdownTable) -> String {
        let headerHTML = zip(table.header, table.alignments).map { cell, alignment in
            "<th\(alignmentStyle(for: alignment))>\(renderInlineMarkdown(cell))</th>"
        }.joined()

        let bodyHTML = table.rows.map { row in
            let cells = zip(row, table.alignments).map { cell, alignment in
                "<td\(alignmentStyle(for: alignment))>\(renderInlineMarkdown(cell))</td>"
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

    private static func alignmentStyle(for alignment: MarkdownTableAlignment?) -> String {
        guard let alignment else { return "" }
        return " style=\"text-align: \(alignment.rawValue);\""
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

    private static func renderMermaidBlockHTML(_ code: String) -> String {
        """
        <div class="mermaid-block">
          <div class="code-label">Mermaid</div>
          <pre><code class="language-mermaid">\(escapeHTML(code))</code></pre>
          <div class="mermaid-note">Diagram execution is not wired yet in the standalone preview. The fence is preserved and styled so conformance stays visible.</div>
        </div>
        """
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
            if let commentPrefix, line[index...].hasPrefix(commentPrefix) {
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
        case "mermaid": return "mermaid"
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

    static func renderInlineMarkdown(_ text: String) -> String {
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

    static func escapeHTML(_ value: String) -> String {
        value
            .replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
            .replacingOccurrences(of: "\"", with: "&quot;")
    }

    static func cssHex(_ color: NSColor) -> String {
        let rgb = color.usingColorSpace(.deviceRGB) ?? color
        let r = Int((rgb.redComponent * 255).rounded())
        let g = Int((rgb.greenComponent * 255).rounded())
        let b = Int((rgb.blueComponent * 255).rounded())
        return String(format: "#%02X%02X%02X", r, g, b)
    }
}
