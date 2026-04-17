import Foundation

enum MarkdownBlock {
    case paragraph(String)
    case heading(level: Int, text: String)
    case thematicBreak
    case blockquote([MarkdownBlock])
    case callout(kind: String, title: String, blocks: [MarkdownBlock])
    case list(MarkdownList)
    case codeFence(language: String?, code: String)
    case mermaid(code: String)
    case table(MarkdownTable)
}

struct MarkdownList {
    let kind: MarkdownListKind
    let items: [MarkdownListItem]
}

enum MarkdownListKind: Equatable {
    case unordered
    case ordered
    case task
}

struct MarkdownListItem {
    let text: String
    let isChecked: Bool?
    let childBlocks: [MarkdownBlock]
}

struct MarkdownTable {
    let header: [String]
    let alignments: [MarkdownTableAlignment?]
    let rows: [[String]]
}

enum MarkdownTableAlignment: String {
    case left
    case center
    case right
}
