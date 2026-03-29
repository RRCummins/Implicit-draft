//
//  AppDelegate.swift
//  implicit
//
//  Created by Ryan Cummins on 3/28/26.
//

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
        flushPendingOpenURLs()
    }

    func application(_ sender: NSApplication, openFiles filenames: [String]) {
        let urls = filenames.map(URL.init(fileURLWithPath:))
        if let mainViewController {
            mainViewController.applicationOpenFiles(urls)
        } else {
            pendingOpenURLs.append(contentsOf: urls)
        }
        sender.reply(toOpenOrPrint: .success)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag {
            NSApp.windows.first?.makeKeyAndOrderFront(self)
        }
        if NSApp.windows.first == nil {
            mainViewController?.applicationCreateNewDocument()
        }
        return true
    }

    func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
        true
    }

    private var mainViewController: ViewController? {
        NSApp.windows.first?.contentViewController as? ViewController
    }

    private func flushPendingOpenURLs() {
        guard !pendingOpenURLs.isEmpty, let mainViewController else { return }
        mainViewController.applicationOpenFiles(pendingOpenURLs)
        pendingOpenURLs.removeAll()
    }
}
