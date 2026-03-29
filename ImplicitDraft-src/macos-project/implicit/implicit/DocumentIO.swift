import Foundation

enum StandaloneDocumentIO {
    static func loadText(from url: URL) throws -> String {
        try String(contentsOf: url, encoding: .utf8)
    }

    static func loadDocument(from url: URL) throws -> EditorDocument {
        EditorDocument(
            id: UUID(),
            url: url,
            title: url.lastPathComponent,
            text: try loadText(from: url),
            isDirty: false,
            diskModificationTime: fileModificationTime(for: url),
            scrollOffset: 0,
            selectionLocation: 0,
            selectionLength: 0,
            mode: .source
        )
    }

    static func reloadDocument(_ doc: inout EditorDocument) throws {
        guard let url = doc.url else { return }
        doc.text = try loadText(from: url)
        doc.isDirty = false
        doc.diskModificationTime = fileModificationTime(for: url)
        doc.scrollOffset = 0
        doc.selectionLocation = 0
        doc.selectionLength = 0
        doc.mode = .source
    }

    static func revertDocument(_ doc: inout EditorDocument) throws {
        guard let url = doc.url else { return }
        doc.text = try loadText(from: url)
        doc.isDirty = false
        doc.diskModificationTime = fileModificationTime(for: url)
        doc.selectionLocation = 0
        doc.selectionLength = 0
        doc.scrollOffset = 0
    }

    static func writeDocument(_ doc: inout EditorDocument, to url: URL) throws -> UUID {
        try doc.text.write(to: url, atomically: true, encoding: .utf8)
        let docID = doc.id
        doc.url = url
        doc.title = url.lastPathComponent
        doc.isDirty = false
        doc.diskModificationTime = fileModificationTime(for: url)
        return docID
    }

    static func saveConflictURL(for doc: EditorDocument) -> URL? {
        guard let url = doc.url,
              let knownTime = doc.diskModificationTime,
              let currentTime = fileModificationTime(for: url) else {
            return nil
        }
        return abs(currentTime - knownTime) > 0.5 ? url : nil
    }

    static func suggestedFilename(for doc: EditorDocument) -> String {
        doc.url?.lastPathComponent ?? (doc.title == "Untitled" ? "Untitled.md" : doc.title)
    }

    static func fileModificationTime(for url: URL) -> TimeInterval? {
        let values = try? url.resourceValues(forKeys: [.contentModificationDateKey])
        return values?.contentModificationDate?.timeIntervalSince1970
    }
}
