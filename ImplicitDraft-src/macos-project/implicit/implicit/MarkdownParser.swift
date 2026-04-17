import Foundation

enum MarkdownParser {
    static func parse(_ markdown: String) -> [MarkdownBlock] {
        let lines = markdown.components(separatedBy: .newlines)
        return parseBlocks(from: lines, startIndex: 0, baseIndent: 0).blocks
    }

    private static func parseBlocks(from lines: [String], startIndex: Int, baseIndent: Int)
        -> (blocks: [MarkdownBlock], nextIndex: Int)
    {
        var blocks: [MarkdownBlock] = []
        var paragraph: [String] = []
        var lineIndex = startIndex

        func flushParagraph() {
            guard !paragraph.isEmpty else { return }
            blocks.append(.paragraph(paragraph.joined(separator: " ")))
            paragraph.removeAll()
        }

        while lineIndex < lines.count {
            let rawLine = lines[lineIndex]
            if rawLine.trimmingCharacters(in: .whitespaces).isEmpty {
                flushParagraph()
                lineIndex += 1
                continue
            }

            let currentIndent = leadingIndentWidth(of: rawLine)
            if currentIndent < baseIndent {
                break
            }

            let scopedLine = dropIndent(rawLine, amount: baseIndent)
            let trimmed = scopedLine.trimmingCharacters(in: .whitespaces)

            if scopedLine.hasPrefix("```") {
                flushParagraph()
                let fence = parseCodeFence(lines: lines, startIndex: lineIndex, baseIndent: baseIndent)
                blocks.append(fence.block)
                lineIndex = fence.nextIndex
                continue
            }

            if (trimmed.dropPrefixIfPresent("> ") ?? trimmed.dropPrefixIfPresent(">")) != nil {
                flushParagraph()
                let blockquote = parseBlockquote(lines: lines, startIndex: lineIndex, baseIndent: baseIndent)
                blocks.append(blockquote)
                lineIndex = parseBlockquoteEnd(lines: lines, startIndex: lineIndex, baseIndent: baseIndent)
                continue
            }

            if let table = parseMarkdownTable(lines: lines, startingAt: lineIndex, baseIndent: baseIndent) {
                flushParagraph()
                blocks.append(.table(table.table))
                lineIndex = table.nextIndex
                continue
            }

            if trimmed == "---" || trimmed == "***" {
                flushParagraph()
                blocks.append(.thematicBreak)
                lineIndex += 1
                continue
            }

            if let heading = parseHeading(trimmed) {
                flushParagraph()
                blocks.append(.heading(level: heading.level, text: heading.text))
                lineIndex += 1
                continue
            }

            if parseListMarker(scopedLine) != nil {
                flushParagraph()
                let list = parseList(lines: lines, startIndex: lineIndex, baseIndent: baseIndent)
                blocks.append(.list(list.block))
                lineIndex = list.nextIndex
                continue
            }

            paragraph.append(trimmed)
            lineIndex += 1
        }

        flushParagraph()
        return (blocks, lineIndex)
    }

    private static func parseCodeFence(lines: [String], startIndex: Int, baseIndent: Int)
        -> (block: MarkdownBlock, nextIndex: Int)
    {
        let opening = dropIndent(lines[startIndex], amount: baseIndent)
        let fenceLanguage = String(opening.dropFirst(3)).trimmingCharacters(in: .whitespaces)
        var codeLines: [String] = []
        var lineIndex = startIndex + 1

        while lineIndex < lines.count {
            let scopedLine = dropIndent(lines[lineIndex], amount: baseIndent)
            if scopedLine.hasPrefix("```") {
                lineIndex += 1
                break
            }
            codeLines.append(scopedLine)
            lineIndex += 1
        }

        let code = codeLines.joined(separator: "\n")
        if fenceLanguage.lowercased() == "mermaid" {
            return (.mermaid(code: code), lineIndex)
        }

        return (.codeFence(language: fenceLanguage.isEmpty ? nil : fenceLanguage, code: code), lineIndex)
    }

    private static func parseBlockquote(lines: [String], startIndex: Int, baseIndent: Int) -> MarkdownBlock {
        var quoteLines: [String] = []
        var lineIndex = startIndex

        while lineIndex < lines.count {
            let rawLine = lines[lineIndex]
            if rawLine.trimmingCharacters(in: .whitespaces).isEmpty {
                quoteLines.append("")
                lineIndex += 1
                continue
            }

            let currentIndent = leadingIndentWidth(of: rawLine)
            if currentIndent < baseIndent {
                break
            }

            let scopedLine = dropIndent(rawLine, amount: baseIndent)
            let trimmed = scopedLine.trimmingCharacters(in: .whitespaces)
            guard let quoteBody = trimmed.dropPrefixIfPresent("> ") ?? trimmed.dropPrefixIfPresent(">") else {
                break
            }
            quoteLines.append(quoteBody)
            lineIndex += 1
        }

        if let callout = parseCallout(quoteLines.first ?? "") {
            var bodyLines = Array(quoteLines.dropFirst())
            if !callout.inlineBody.isEmpty {
                bodyLines.insert(callout.inlineBody, at: 0)
            }
            return .callout(kind: callout.kind, title: callout.title, blocks: parse(bodyLines.joined(separator: "\n")))
        }

        return .blockquote(parse(quoteLines.joined(separator: "\n")))
    }

    private static func parseBlockquoteEnd(lines: [String], startIndex: Int, baseIndent: Int) -> Int {
        var lineIndex = startIndex
        while lineIndex < lines.count {
            let rawLine = lines[lineIndex]
            if rawLine.trimmingCharacters(in: .whitespaces).isEmpty {
                lineIndex += 1
                continue
            }

            let currentIndent = leadingIndentWidth(of: rawLine)
            if currentIndent < baseIndent {
                break
            }

            let scopedLine = dropIndent(rawLine, amount: baseIndent)
            let trimmed = scopedLine.trimmingCharacters(in: .whitespaces)
            guard trimmed.hasPrefix(">") else { break }
            lineIndex += 1
        }
        return lineIndex
    }

    private static func parseList(lines: [String], startIndex: Int, baseIndent: Int)
        -> (block: MarkdownList, nextIndex: Int)
    {
        let firstScopedLine = dropIndent(lines[startIndex], amount: baseIndent)
        guard let firstMarker = parseListMarker(firstScopedLine) else {
            return (MarkdownList(kind: .unordered, items: []), startIndex)
        }

        let listIndent = firstMarker.indent
        let listKind = firstMarker.kind
        var items: [MarkdownListItem] = []
        var lineIndex = startIndex

        while lineIndex < lines.count {
            let rawLine = lines[lineIndex]
            if rawLine.trimmingCharacters(in: .whitespaces).isEmpty {
                lineIndex += 1
                break
            }

            let scopedLine = dropIndent(rawLine, amount: baseIndent)
            guard let marker = parseListMarker(scopedLine),
                  marker.indent == listIndent,
                  marker.kind == listKind else {
                break
            }

            var childSource: [String] = []
            lineIndex += 1

            while lineIndex < lines.count {
                let nextRaw = lines[lineIndex]
                if nextRaw.trimmingCharacters(in: .whitespaces).isEmpty {
                    childSource.append("")
                    lineIndex += 1
                    continue
                }

                let nextScoped = dropIndent(nextRaw, amount: baseIndent)
                let nextIndent = leadingIndentWidth(of: nextScoped)

                if let nextMarker = parseListMarker(nextScoped),
                   nextMarker.indent == listIndent,
                   nextMarker.kind == listKind {
                    break
                }

                if nextIndent <= listIndent {
                    break
                }

                childSource.append(dropIndent(nextScoped, amount: min(marker.contentIndent, nextIndent)))
                lineIndex += 1
            }

            let childBlocks = parse(childSource.joined(separator: "\n"))
            items.append(MarkdownListItem(text: marker.text, isChecked: marker.isChecked, childBlocks: childBlocks))
        }

        return (MarkdownList(kind: listKind, items: items), lineIndex)
    }

    private static func parseMarkdownTable(lines: [String], startingAt index: Int, baseIndent: Int)
        -> (table: MarkdownTable, nextIndex: Int)?
    {
        guard index + 1 < lines.count,
              let header = splitMarkdownTableRow(dropIndent(lines[index], amount: baseIndent)),
              let alignments = parseMarkdownTableSeparator(dropIndent(lines[index + 1], amount: baseIndent)),
              header.count == alignments.count else {
            return nil
        }

        var rows: [[String]] = []
        var nextIndex = index + 2

        while nextIndex < lines.count {
            let rawLine = dropIndent(lines[nextIndex], amount: baseIndent)
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

        return (MarkdownTable(header: header, alignments: alignments, rows: rows), nextIndex)
    }

    private static func splitMarkdownTableRow(_ rawLine: String) -> [String]? {
        let trimmed = rawLine.trimmingCharacters(in: .whitespaces)
        guard trimmed.contains("|") else { return nil }

        let core = trimmed.trimmingCharacters(in: CharacterSet(charactersIn: "|"))
        let parts = core.split(separator: "|", omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespaces) }
        return parts.isEmpty ? nil : parts
    }

    private static func parseMarkdownTableSeparator(_ rawLine: String) -> [MarkdownTableAlignment?]? {
        guard let parts = splitMarkdownTableRow(rawLine) else { return nil }

        var alignments: [MarkdownTableAlignment?] = []
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
                alignments.append(.center)
            } else if trailingColon {
                alignments.append(.right)
            } else if leadingColon {
                alignments.append(.left)
            } else {
                alignments.append(nil)
            }
        }

        return alignments
    }

    private static func parseHeading(_ line: String) -> (level: Int, text: String)? {
        let hashes = line.prefix { $0 == "#" }
        guard (1...6).contains(hashes.count), line.dropFirst(hashes.count).hasPrefix(" ") else {
            return nil
        }
        let text = line.dropFirst(hashes.count).trimmingCharacters(in: .whitespaces)
        return (hashes.count, text)
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
        let title = String(line[bodyRange]).trimmingCharacters(in: .whitespaces)
        return (kind, title, "")
    }

    private static func parseTaskListItem(_ line: String) -> (completed: Bool, text: String, prefixLength: Int)? {
        let prefixes = ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] ", "+ [ ] ", "+ [x] ", "+ [X] "]
        for prefix in prefixes where line.hasPrefix(prefix) {
            let completed = prefix.lowercased().contains("[x]")
            return (completed, String(line.dropFirst(prefix.count)), prefix.count)
        }
        return nil
    }

    private static func parseListMarker(_ rawLine: String) -> ParsedListMarker? {
        let indent = leadingIndentWidth(of: rawLine)
        let trimmedLine = rawLine.trimmingCharacters(in: .whitespaces)

        if let task = parseTaskListItem(trimmedLine) {
            return ParsedListMarker(
                indent: indent,
                contentIndent: indent + task.prefixLength,
                kind: .task,
                text: task.text,
                isChecked: task.completed
            )
        }

        if let item = trimmedLine.dropPrefixIfPresent("- ")
            ?? trimmedLine.dropPrefixIfPresent("* ")
            ?? trimmedLine.dropPrefixIfPresent("+ ") {
            return ParsedListMarker(
                indent: indent,
                contentIndent: indent + 2,
                kind: .unordered,
                text: item,
                isChecked: nil
            )
        }

        if let ordered = trimmedLine.captureOrderedListItemWithPrefixLength() {
            return ParsedListMarker(
                indent: indent,
                contentIndent: indent + ordered.prefixLength,
                kind: .ordered,
                text: ordered.text,
                isChecked: nil
            )
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

    private static func dropIndent(_ rawLine: String, amount: Int) -> String {
        guard amount > 0 else { return rawLine }
        var index = rawLine.startIndex
        var remaining = amount
        while index < rawLine.endIndex && remaining > 0 {
            let char = rawLine[index]
            switch char {
            case " ":
                remaining -= 1
                index = rawLine.index(after: index)
            case "\t":
                remaining -= min(4, remaining)
                index = rawLine.index(after: index)
            default:
                return String(rawLine[index...])
            }
        }
        return String(rawLine[index...])
    }
}

private struct ParsedListMarker {
    let indent: Int
    let contentIndent: Int
    let kind: MarkdownListKind
    let text: String
    let isChecked: Bool?
}

private extension String {
    func dropPrefixIfPresent(_ prefix: String) -> String? {
        hasPrefix(prefix) ? String(dropFirst(prefix.count)) : nil
    }

    func captureOrderedListItemWithPrefixLength() -> (text: String, prefixLength: Int)? {
        guard let dot = firstIndex(of: ".") else { return nil }
        let prefix = self[..<dot]
        guard !prefix.isEmpty, prefix.allSatisfy(\.isNumber) else { return nil }
        let remainderStart = index(after: dot)
        guard remainderStart < endIndex, self[remainderStart] == " " else { return nil }
        let contentStart = index(after: remainderStart)
        guard contentStart <= endIndex else { return nil }
        let text = String(self[contentStart...])
        let prefixLength = distance(from: startIndex, to: contentStart)
        return (text, prefixLength)
    }
}
