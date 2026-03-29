// Session.swift
// Persists open tabs and editor state across launches.
// Stored in Application Support so it works inside the app sandbox.

import Foundation

enum EditorMode: Int, Codable {
    case source = 0
    case preview = 1
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
