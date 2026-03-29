// AppDelegate.swift

import Cocoa

@main
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var pendingOpenURLs: [URL] = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard let window = NSApp.windows.first else { return }
        window.setContentSize(NSSize(width: 1320, height: 860))
        window.minSize = NSSize(width: 920, height: 620)
        window.center()
        window.appearance = NSAppearance(named: .darkAqua)
        window.makeKeyAndOrderFront(self)
        NSApp.activate(ignoringOtherApps: true)
        flushPendingOpenURLs()
    }

    func application(_ sender: NSApplication, openFiles filenames: [String]) {
        let urls = filenames.map(URL.init(fileURLWithPath:))
        if let vc = mainViewController {
            vc.applicationOpenFiles(urls)
        } else {
            pendingOpenURLs.append(contentsOf: urls)
        }
        sender.reply(toOpenOrPrint: .success)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag { NSApp.windows.first?.makeKeyAndOrderFront(self) }
        if NSApp.windows.first == nil { mainViewController?.applicationCreateNewDocument() }
        return true
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let vc = mainViewController, vc.hasDirtyDocuments else {
            return .terminateNow
        }
        vc.confirmTermination { allow in
            NSApp.reply(toApplicationShouldTerminate: allow)
        }
        return .terminateLater
    }

    func applicationWillTerminate(_ notification: Notification) {
        mainViewController?.saveSession()
    }

    func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool { true }

    // MARK: Private

    private var mainViewController: ViewController? {
        NSApp.windows.first?.contentViewController as? ViewController
    }

    private func flushPendingOpenURLs() {
        guard !pendingOpenURLs.isEmpty, let vc = mainViewController else { return }
        vc.applicationOpenFiles(pendingOpenURLs)
        pendingOpenURLs.removeAll()
    }
}
