// Session.swift
// Persists open tabs and editor state across launches.
// Stored in Application Support so it works inside the app sandbox.

import Foundation

enum EditorMode: Int, Codable {
    case source = 0
    case preview = 1
    case live = 2

    var footerSegmentIndex: Int {
        switch self {
        case .source:
            return 0
        case .live:
            return 1
        case .preview:
            return 2
        }
    }

    static func fromFooterSegmentIndex(_ index: Int) -> EditorMode {
        switch index {
        case 1:
            return .live
        case 2:
            return .preview
        default:
            return .source
        }
    }
}

struct EditorDocument {
    let id: UUID
    var url: URL?
    var title: String
    var text: String
    var isDirty: Bool
    var diskModificationTime: TimeInterval?
    var scrollOffset: Double
    var selectionLocation: Int
    var selectionLength: Int
    var mode: EditorMode

    static func untitled() -> EditorDocument {
        EditorDocument(
            id: UUID(),
            url: nil,
            title: "Untitled",
            text: "",
            isDirty: false,
            diskModificationTime: nil,
            scrollOffset: 0,
            selectionLocation: 0,
            selectionLength: 0,
            mode: .source
        )
    }

    var displayTitle: String { isDirty ? "\(title) ●" : title }
}

// MARK: - Codable document snapshot

struct SavedDocument: Codable {
    let id: UUID
    var urlPath: String?  // nil for untitled
    var title: String
    var text: String?     // nil for clean saved-to-disk documents (re-read on restore)
    var isDirty: Bool
    var diskModificationTime: TimeInterval?
    var scrollOffset: Double
    var selectionLocation: Int
    var selectionLength: Int
    var modeRawValue: Int
}

// MARK: - Codable session snapshot

struct SavedSession: Codable {
    var documents: [SavedDocument]
    var selectedDocumentID: UUID?
}

// MARK: - Store

enum SessionStore {
    // MARK: Paths

    /// ~/Library/Application Support/Implicit/ (sandbox-safe)
    static var supportDir: URL {
        let base = FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask
        ).first!
        let dir = base.appendingPathComponent("Implicit", isDirectory: true)
        try? FileManager.default.createDirectory(
            at: dir, withIntermediateDirectories: true
        )
        return dir
    }

    static var sessionURL: URL {
        supportDir.appendingPathComponent("session.json")
    }

    static var recoveryDir: URL {
        let dir = supportDir.appendingPathComponent("recovery", isDirectory: true)
        try? FileManager.default.createDirectory(
            at: dir, withIntermediateDirectories: true
        )
        return dir
    }

    // MARK: Session save / load

    static func save(_ session: SavedSession) {
        guard let data = try? JSONEncoder().encode(session) else { return }
        try? data.write(to: sessionURL, options: .atomic)
    }

    static func load() -> SavedSession? {
        guard let data = try? Data(contentsOf: sessionURL) else { return nil }
        return try? JSONDecoder().decode(SavedSession.self, from: data)
    }

    // MARK: Recovery files

    /// Writes the current text for an unsaved/dirty document to the recovery directory.
    /// Called on every edit (should be debounced by the caller).
    static func writeRecovery(id: UUID, text: String) {
        let url = recoveryDir.appendingPathComponent("\(id.uuidString).md")
        try? text.write(to: url, atomically: true, encoding: .utf8)
    }

    /// Removes the recovery file for a document once it has been saved to its real URL.
    static func clearRecovery(id: UUID) {
        let url = recoveryDir.appendingPathComponent("\(id.uuidString).md")
        try? FileManager.default.removeItem(at: url)
    }

    /// Returns the recovery text for a document ID if a recovery file exists.
    static func loadRecovery(id: UUID) -> String? {
        let url = recoveryDir.appendingPathComponent("\(id.uuidString).md")
        return try? String(contentsOf: url, encoding: .utf8)
    }
}

@MainActor
final class StandaloneSession {
    var documents: [EditorDocument] = []
    var selectedDocumentID: UUID?

    var hasDirtyDocuments: Bool {
        documents.contains { $0.isDirty }
    }

    var dirtyDocumentCount: Int {
        documents.filter(\.isDirty).count
    }

    var dirtyDocumentIndices: [Int] {
        documents.indices.filter { documents[$0].isDirty }
    }

    var pristineSeedDocumentIndex: Int? {
        guard documents.count == 1 else { return nil }
        let doc = documents[0]
        guard doc.url == nil,
              doc.text.isEmpty,
              !doc.isDirty,
              doc.selectionLocation == 0,
              doc.selectionLength == 0,
              doc.scrollOffset == 0 else {
            return nil
        }
        return 0
    }

    func selectedDocumentIndex() -> Int? {
        guard let id = selectedDocumentID else { return nil }
        return documents.firstIndex { $0.id == id }
    }

    func activateDocument(at index: Int) {
        guard documents.indices.contains(index) else { return }
        selectedDocumentID = documents[index].id
    }

    func createUntitled(atStart: Bool = false) -> EditorDocument {
        let doc = EditorDocument.untitled()
        if atStart {
            documents.insert(doc, at: 0)
        } else {
            documents.append(doc)
        }
        selectedDocumentID = doc.id
        return doc
    }

    func closeDocument(at index: Int) {
        guard documents.indices.contains(index) else { return }
        let wasSelected = documents[index].id == selectedDocumentID
        documents.remove(at: index)

        if documents.isEmpty {
            let doc = EditorDocument.untitled()
            documents = [doc]
            selectedDocumentID = doc.id
        } else if wasSelected {
            let newIndex = max(0, min(index, documents.count - 1))
            selectedDocumentID = documents[newIndex].id
        }
    }

    func restoreOrSeed() {
        guard let session = SessionStore.load(), !session.documents.isEmpty else {
            seedInitialDocumentIfNeeded()
            return
        }

        let restored = session.documents.compactMap(restoredDocument(from:))
        guard !restored.isEmpty else {
            seedInitialDocumentIfNeeded()
            return
        }

        documents = restored
        selectedDocumentID = session.selectedDocumentID
            .flatMap { id in restored.first(where: { $0.id == id })?.id }
            ?? restored.first?.id
    }

    func seedInitialDocumentIfNeeded() {
        guard documents.isEmpty else { return }
        let doc = EditorDocument.untitled()
        documents = [doc]
        selectedDocumentID = doc.id
    }

    func save() {
        let saved = documents.map { doc -> SavedDocument in
            let embedText = doc.isDirty || doc.url == nil
            return SavedDocument(
                id: doc.id,
                urlPath: doc.url?.path(percentEncoded: false),
                title: doc.title,
                text: embedText ? doc.text : nil,
                isDirty: doc.isDirty,
                diskModificationTime: doc.diskModificationTime,
                scrollOffset: doc.scrollOffset,
                selectionLocation: doc.selectionLocation,
                selectionLength: doc.selectionLength,
                modeRawValue: doc.mode.rawValue
            )
        }

        SessionStore.save(SavedSession(
            documents: saved,
            selectedDocumentID: selectedDocumentID
        ))
    }

    private func restoredDocument(from saved: SavedDocument) -> EditorDocument? {
        let recoveryText = SessionStore.loadRecovery(id: saved.id)

        if let path = saved.urlPath {
            let url = URL(fileURLWithPath: path)
            if !saved.isDirty, let text = try? String(contentsOf: url, encoding: .utf8) {
                return EditorDocument(
                    id: saved.id,
                    url: url,
                    title: saved.title,
                    text: text,
                    isDirty: false,
                    diskModificationTime: fileModificationTime(for: url),
                    scrollOffset: saved.scrollOffset,
                    selectionLocation: saved.selectionLocation,
                    selectionLength: saved.selectionLength,
                    mode: EditorMode(rawValue: saved.modeRawValue) ?? .source
                )
            }

            let text = recoveryText ?? saved.text ?? ""
            return EditorDocument(
                id: saved.id,
                url: url,
                title: saved.title,
                text: text,
                isDirty: !text.isEmpty || saved.isDirty,
                diskModificationTime: saved.diskModificationTime ?? fileModificationTime(for: url),
                scrollOffset: saved.scrollOffset,
                selectionLocation: saved.selectionLocation,
                selectionLength: saved.selectionLength,
                mode: EditorMode(rawValue: saved.modeRawValue) ?? .source
            )
        }

        let text = recoveryText ?? saved.text ?? ""
        return EditorDocument(
            id: saved.id,
            url: nil,
            title: saved.title,
            text: text,
            isDirty: text.isEmpty ? false : saved.isDirty,
            diskModificationTime: nil,
            scrollOffset: saved.scrollOffset,
            selectionLocation: saved.selectionLocation,
            selectionLength: saved.selectionLength,
            mode: EditorMode(rawValue: saved.modeRawValue) ?? .source
        )
    }

    private func fileModificationTime(for url: URL) -> TimeInterval? {
        let values = try? url.resourceValues(forKeys: [.contentModificationDateKey])
        return values?.contentModificationDate?.timeIntervalSince1970
    }
}
