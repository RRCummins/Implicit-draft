import Cocoa

enum SaveConflictResolution {
    case overwrite
    case saveAs
    case cancel
}

enum LossyCloseResolution {
    case save
    case discard
    case cancel
}

enum StandaloneDocumentCoordinator {
    static func requestSaveURL(
        window: NSWindow,
        suggestedFilename: String,
        completion: @escaping (URL?) -> Void
    ) {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = suggestedFilename
        panel.canCreateDirectories = true
        panel.beginSheetModal(for: window) { response in
            completion(response == .OK ? panel.url : nil)
        }
    }

    static func requestConflictResolution(
        window: NSWindow,
        url: URL,
        completion: @escaping (SaveConflictResolution) -> Void
    ) {
        let alert = NSAlert()
        alert.messageText = "File changed on disk"
        alert.informativeText = "\"\(url.lastPathComponent)\" was modified outside Implicit. Overwrite it, or save your buffer somewhere else."
        alert.addButton(withTitle: "Overwrite")
        alert.addButton(withTitle: "Save As")
        alert.addButton(withTitle: "Cancel")
        alert.alertStyle = .warning
        alert.beginSheetModal(for: window) { response in
            switch response {
            case .alertFirstButtonReturn:
                completion(.overwrite)
            case .alertSecondButtonReturn:
                completion(.saveAs)
            default:
                completion(.cancel)
            }
        }
    }

    static func requestLossyClose(
        window: NSWindow,
        title: String,
        completion: @escaping (LossyCloseResolution) -> Void
    ) {
        let alert = NSAlert()
        alert.messageText = "Save \"\(title)\"?"
        alert.informativeText = "Your changes will be lost if you don't save before closing."
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Don't Save")
        alert.addButton(withTitle: "Cancel")
        alert.alertStyle = .warning
        alert.beginSheetModal(for: window) { response in
            switch response {
            case .alertFirstButtonReturn:
                completion(.save)
            case .alertSecondButtonReturn:
                completion(.discard)
            default:
                completion(.cancel)
            }
        }
    }
}
