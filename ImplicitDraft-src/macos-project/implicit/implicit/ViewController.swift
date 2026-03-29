// ViewController.swift
// Single-window flat layout — Phase 2: session persistence.

import Cocoa

// MARK: - Data model

private struct EditorDocument {
    let id: UUID
    var url: URL?
    var title: String
    var text: String
    var isDirty: Bool

    static func untitled() -> EditorDocument {
        EditorDocument(id: UUID(), url: nil, title: "Untitled", text: "", isDirty: false)
    }

    var displayTitle: String { isDirty ? "\(title) ●" : title }
}

private enum EditorMode: Int {
    case source = 0
    case preview = 1
}

// Unified sidebar row model. Section header rows are not selectable.
private enum SidebarRow {
    case header(String)
    case openDoc(Int)    // index into documents
    case recent(URL)
}

// MARK: - ViewController

final class ViewController: NSViewController,
                             NSTableViewDataSource, NSTableViewDelegate,
                             NSTextViewDelegate, NSMenuItemValidation {
    // MARK: Subviews
    private let splitView          = FlatSplitView()
    private let sidebarTable       = NSTableView(frame: .zero)
    private let editorTextView     = NSTextView(frame: .zero)
    private let editorScrollView   = NSScrollView()
    private let tabStripStack      = NSStackView()
    private let statusLabel        = NSTextField(labelWithString: "Untitled draft")
    private let statusMetaLabel    = NSTextField(labelWithString: "Source · Markdown · Saved")
    private let modeControl        = NSSegmentedControl(
        labels: ["Source", "Preview"], trackingMode: .selectOne, target: nil, action: nil
    )
    private let emptyContainer     = NSView()
    private let emptyTitleLabel    = NSTextField(labelWithString: "Start a draft")
    private let emptyBodyLabel     = NSTextField(
        labelWithString: "Write immediately, or open an existing file."
    )
    private let emptyOpenButton    = NSButton(title: "Open File →", target: nil, action: nil)

    // MARK: State
    private var documents:            [EditorDocument] = []
    private var selectedDocumentID:   UUID?
    private var sidebarRows:          [SidebarRow] = []
    private var isSwitchingDocuments: Bool = false
    private var mode:                 EditorMode = .source
    private var didSetInitialSplit:   Bool = false
    private var theme:                ImplicitTheme = ImplicitThemeLoader.load()
    private var recoveryTimer:        Timer?

    // MARK: Public interface for AppDelegate

    var hasDirtyDocuments: Bool { documents.contains { $0.isDirty } }
    var dirtyDocumentCount: Int { documents.filter(\.isDirty).count }

    // MARK: Lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()
        buildInterface()
        restoreSessionOrSeed()
        updateVisibleDocument()
        NotificationCenter.default.addObserver(
            self, selector: #selector(appWillResignActive),
            name: NSApplication.willResignActiveNotification, object: nil
        )
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        guard let window = view.window else { return }
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.isMovableByWindowBackground = true
        window.backgroundColor = AppPalette.windowBg
        window.appearance = NSAppearance(named: .darkAqua)
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        guard !didSetInitialSplit, splitView.subviews.count > 1 else { return }
        splitView.setPosition(AppMetrics.sidebarWidth, ofDividerAt: 0)
        didSetInitialSplit = true
    }

    // MARK: App delegate bridge

    func applicationOpenFiles(_ urls: [URL]) { openDocuments(urls) }
    func applicationCreateNewDocument()      { newDocument(nil) }

    override var representedObject: Any? { didSet {} }

    // MARK: Actions

    @IBAction func newDocument(_ sender: Any?) {
        let doc = EditorDocument.untitled()
        documents.insert(doc, at: 0)
        selectedDocumentID = doc.id
        refreshAll()
    }

    @IBAction func openDocument(_ sender: Any?) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.allowsMultipleSelection = true
        panel.beginSheetModal(for: view.window!) { [weak self] response in
            guard response == .OK else { return }
            self?.openDocuments(panel.urls)
        }
    }

    @IBAction func saveDocument(_ sender: Any?) {
        guard let index = selectedDocumentIndex else { return }
        if let url = documents[index].url {
            writeDocument(at: index, to: url)
        } else {
            saveDocumentAs(sender)
        }
    }

    @IBAction func saveDocumentAs(_ sender: Any?) {
        guard let index = selectedDocumentIndex else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = suggestedFilename(for: documents[index])
        panel.canCreateDirectories = true
        panel.beginSheetModal(for: view.window!) { [weak self] response in
            guard response == .OK, let url = panel.url else { return }
            self?.writeDocument(at: index, to: url)
        }
    }

    @IBAction func revertDocumentToSaved(_ sender: Any?) {
        guard let index = selectedDocumentIndex, let url = documents[index].url else { return }
        do {
            let text = try String(contentsOf: url, encoding: .utf8)
            documents[index].text = text
            documents[index].isDirty = false
            statusLabel.stringValue = "Reverted \(documents[index].title)"
            refreshAll()
        } catch {
            statusLabel.stringValue = "Error: \(error.localizedDescription)"
        }
    }

    @IBAction func changeMode(_ sender: Any?) {
        mode = EditorMode(rawValue: modeControl.selectedSegment) ?? .source
        updateVisibleDocument()
    }

    // MARK: Menu validation

    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        switch menuItem.action {
        case #selector(saveDocument(_:)),
             #selector(saveDocumentAs(_:)),
             #selector(revertDocumentToSaved(_:)):
            return selectedDocumentIndex != nil
        default:
            return true
        }
    }

    // MARK: NSTableViewDataSource

    func numberOfRows(in tableView: NSTableView) -> Int { sidebarRows.count }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        switch sidebarRows[row] {
        case .header(let title):
            return makeSidebarHeaderCell(title: title)
        case .openDoc(let index):
            let doc = documents[index]
            let isActive = doc.id == selectedDocumentID
            return makeSidebarDocCell(title: doc.displayTitle, isActive: isActive)
        case .recent(let url):
            return makeSidebarRecentCell(url: url)
        }
    }

    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        switch sidebarRows[row] {
        case .header: return TransparentTableRowView()
        default:      return FlatTableRowView()
        }
    }

    // MARK: NSTableViewDelegate

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        switch sidebarRows[row] {
        case .header: return 28
        default:      return AppMetrics.sidebarRowHeight
        }
    }

    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool {
        switch sidebarRows[row] {
        case .header: return false
        default:      return true
        }
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        let row = sidebarTable.selectedRow
        guard sidebarRows.indices.contains(row) else { return }
        switch sidebarRows[row] {
        case .header:
            break
        case .openDoc(let index):
            selectedDocumentID = documents[index].id
            updateVisibleDocument()
        case .recent(let url):
            openDocuments([url])
        }
    }

    // MARK: NSTextViewDelegate

    func textDidChange(_ notification: Notification) {
        guard !isSwitchingDocuments, let index = selectedDocumentIndex else { return }
        documents[index].text = editorTextView.string
        documents[index].isDirty = true
        updateWindowTitle()
        rebuildSidebarRows()
        refreshTabStrip()
        updateStatusBar()
        scheduleRecoveryWrite(for: documents[index])
    }

    // MARK: Interface construction

    private func buildInterface() {
        view.wantsLayer = true
        view.layer?.backgroundColor = AppPalette.windowBg.cgColor

        let root = NSStackView()
        root.orientation = .vertical
        root.spacing = 0
        root.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(root)

        let tabBar    = makeTabBar()
        let body      = makeBody()
        let statusBar = makeStatusBar()

        root.addArrangedSubview(tabBar)
        root.addArrangedSubview(body)
        root.addArrangedSubview(statusBar)

        NSLayoutConstraint.activate([
            root.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            root.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            root.topAnchor.constraint(equalTo: view.topAnchor),
            root.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            tabBar.heightAnchor.constraint(equalToConstant: AppMetrics.tabBarHeight),
            statusBar.heightAnchor.constraint(equalToConstant: AppMetrics.statusBarHeight),
        ])
    }

    // MARK: Tab bar

    private func makeTabBar() -> NSView {
        let bar = NSView()
        bar.wantsLayer = true
        bar.layer?.backgroundColor = AppPalette.tabBarBg.cgColor

        tabStripStack.orientation = .horizontal
        tabStripStack.alignment = .centerY
        tabStripStack.spacing = 2
        tabStripStack.translatesAutoresizingMaskIntoConstraints = false
        bar.addSubview(tabStripStack)

        let bottomBorder = NSView()
        bottomBorder.wantsLayer = true
        bottomBorder.layer?.backgroundColor = AppPalette.border.cgColor
        bottomBorder.translatesAutoresizingMaskIntoConstraints = false
        bar.addSubview(bottomBorder)

        NSLayoutConstraint.activate([
            tabStripStack.leadingAnchor.constraint(
                equalTo: bar.leadingAnchor, constant: AppMetrics.tabBarLeadInset
            ),
            tabStripStack.trailingAnchor.constraint(equalTo: bar.trailingAnchor, constant: -8),
            tabStripStack.topAnchor.constraint(equalTo: bar.topAnchor),
            tabStripStack.bottomAnchor.constraint(equalTo: bar.bottomAnchor, constant: -1),
            bottomBorder.leadingAnchor.constraint(equalTo: bar.leadingAnchor),
            bottomBorder.trailingAnchor.constraint(equalTo: bar.trailingAnchor),
            bottomBorder.bottomAnchor.constraint(equalTo: bar.bottomAnchor),
            bottomBorder.heightAnchor.constraint(equalToConstant: 1),
        ])

        return bar
    }

    // MARK: Body

    private func makeBody() -> NSView {
        let container = NSView()
        container.translatesAutoresizingMaskIntoConstraints = false

        splitView.isVertical = true
        splitView.dividerStyle = .thin
        splitView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(splitView)

        let sidebarView = makeSidebar()
        let editorView  = makeEditor()

        splitView.addArrangedSubview(sidebarView)
        splitView.addArrangedSubview(editorView)
        splitView.setHoldingPriority(.defaultLow, forSubviewAt: 0)

        NSLayoutConstraint.activate([
            splitView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            splitView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            splitView.topAnchor.constraint(equalTo: container.topAnchor),
            splitView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            sidebarView.widthAnchor.constraint(equalToConstant: AppMetrics.sidebarWidth),
        ])

        return container
    }

    // MARK: Sidebar

    private func makeSidebar() -> NSView {
        let sidebar = NSView()
        sidebar.wantsLayer = true
        sidebar.layer?.backgroundColor = AppPalette.sidebarBg.cgColor

        sidebarTable.headerView = nil
        sidebarTable.style = .plain
        sidebarTable.focusRingType = .none
        sidebarTable.backgroundColor = .clear
        sidebarTable.intercellSpacing = .zero
        sidebarTable.usesAutomaticRowHeights = false
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("sidebar"))
        sidebarTable.addTableColumn(column)
        sidebarTable.delegate = self
        sidebarTable.dataSource = self

        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = sidebarTable
        sidebar.addSubview(scroll)

        let rightBorder = NSView()
        rightBorder.wantsLayer = true
        rightBorder.layer?.backgroundColor = AppPalette.border.cgColor
        rightBorder.translatesAutoresizingMaskIntoConstraints = false
        sidebar.addSubview(rightBorder)

        NSLayoutConstraint.activate([
            scroll.leadingAnchor.constraint(equalTo: sidebar.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: rightBorder.leadingAnchor),
            scroll.topAnchor.constraint(equalTo: sidebar.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: sidebar.bottomAnchor),
            rightBorder.trailingAnchor.constraint(equalTo: sidebar.trailingAnchor),
            rightBorder.topAnchor.constraint(equalTo: sidebar.topAnchor),
            rightBorder.bottomAnchor.constraint(equalTo: sidebar.bottomAnchor),
            rightBorder.widthAnchor.constraint(equalToConstant: 1),
        ])

        return sidebar
    }

    // MARK: Sidebar cells

    private func makeSidebarHeaderCell(title: String) -> NSView {
        let id = NSUserInterfaceItemIdentifier("SidebarHeader")
        let cell = NSTableCellView()
        cell.identifier = id
        let label = NSTextField(labelWithString: title)
        label.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
        label.textColor = AppPalette.textMuted
        label.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 12),
            label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    private func makeSidebarDocCell(title: String, isActive: Bool) -> NSView {
        let id = NSUserInterfaceItemIdentifier("SidebarDoc")
        let cell = NSTableCellView()
        cell.identifier = id
        let label = NSTextField(labelWithString: title)
        label.font = NSFont.systemFont(ofSize: AppMetrics.bodyFontSize, weight: .regular)
        label.textColor = isActive ? AppPalette.textPrimary : AppPalette.textMuted
        label.lineBreakMode = .byTruncatingMiddle
        label.translatesAutoresizingMaskIntoConstraints = false
        cell.addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 12),
            label.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -8),
            label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
        ])
        return cell
    }

    private func makeSidebarRecentCell(url: URL) -> NSView {
        let id = NSUserInterfaceItemIdentifier("SidebarRecent")
        let cell = NSTableCellView()
        cell.identifier = id

        let name = NSTextField(labelWithString: url.lastPathComponent)
        name.font = NSFont.systemFont(ofSize: AppMetrics.bodyFontSize, weight: .regular)
        name.textColor = AppPalette.textMuted
        name.lineBreakMode = .byTruncatingMiddle
        name.translatesAutoresizingMaskIntoConstraints = false

        let dir = NSTextField(
            labelWithString: url.deletingLastPathComponent().lastPathComponent
        )
        dir.font = NSFont.systemFont(ofSize: 10, weight: .regular)
        dir.textColor = AppPalette.textMuted.withAlphaComponent(0.6)
        dir.lineBreakMode = .byTruncatingHead
        dir.translatesAutoresizingMaskIntoConstraints = false

        cell.addSubview(name)
        cell.addSubview(dir)
        NSLayoutConstraint.activate([
            name.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 12),
            name.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -8),
            name.topAnchor.constraint(equalTo: cell.topAnchor, constant: 5),
            dir.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 12),
            dir.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -8),
            dir.topAnchor.constraint(equalTo: name.bottomAnchor, constant: 1),
        ])
        return cell
    }

    // MARK: Editor

    private func makeEditor() -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.layer?.backgroundColor = AppPalette.windowBg.cgColor

        editorScrollView.hasVerticalScroller = true
        editorScrollView.hasHorizontalScroller = false
        editorScrollView.borderType = .noBorder
        editorScrollView.drawsBackground = false
        editorScrollView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(editorScrollView)

        editorTextView.isRichText = false
        editorTextView.isAutomaticQuoteSubstitutionEnabled = false
        editorTextView.isAutomaticDataDetectionEnabled = false
        editorTextView.isContinuousSpellCheckingEnabled = false
        editorTextView.usesFindBar = true
        editorTextView.font = NSFont.monospacedSystemFont(
            ofSize: AppMetrics.monoFontSize, weight: .regular
        )
        editorTextView.textColor = AppPalette.textPrimary
        editorTextView.backgroundColor = AppPalette.windowBg
        editorTextView.insertionPointColor = AppPalette.accent
        editorTextView.selectedTextAttributes = [
            .backgroundColor: AppPalette.accent.withAlphaComponent(0.25),
        ]
        editorTextView.allowsUndo = true
        editorTextView.textContainerInset = NSSize(
            width: AppMetrics.editorInsetH, height: AppMetrics.editorInsetV
        )
        editorTextView.isHorizontallyResizable = false
        editorTextView.isVerticallyResizable = true
        editorTextView.autoresizingMask = [.width]
        editorTextView.textContainer?.widthTracksTextView = true
        editorTextView.delegate = self
        editorScrollView.documentView = editorTextView

        // Empty state — inline centered, no card border
        emptyContainer.wantsLayer = true
        emptyContainer.layer?.backgroundColor = AppPalette.windowBg.cgColor
        emptyContainer.isHidden = true
        emptyContainer.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(emptyContainer)

        let emptyStack = NSStackView()
        emptyStack.orientation = .vertical
        emptyStack.alignment = .leading
        emptyStack.spacing = 10
        emptyStack.translatesAutoresizingMaskIntoConstraints = false
        emptyContainer.addSubview(emptyStack)

        emptyTitleLabel.font = NSFont.systemFont(ofSize: 20, weight: .semibold)
        emptyTitleLabel.textColor = AppPalette.textPrimary
        emptyBodyLabel.font = NSFont.systemFont(ofSize: AppMetrics.bodyFontSize, weight: .regular)
        emptyBodyLabel.textColor = AppPalette.textMuted
        emptyBodyLabel.maximumNumberOfLines = 3
        emptyBodyLabel.preferredMaxLayoutWidth = 360
        emptyOpenButton.isBordered = false
        emptyOpenButton.font = NSFont.systemFont(ofSize: AppMetrics.bodyFontSize, weight: .medium)
        emptyOpenButton.contentTintColor = AppPalette.accent
        emptyOpenButton.target = self
        emptyOpenButton.action = #selector(openDocument(_:))

        emptyStack.addArrangedSubview(emptyTitleLabel)
        emptyStack.addArrangedSubview(emptyBodyLabel)
        emptyStack.addArrangedSubview(emptyOpenButton)

        NSLayoutConstraint.activate([
            editorScrollView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            editorScrollView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            editorScrollView.topAnchor.constraint(equalTo: container.topAnchor),
            editorScrollView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            emptyContainer.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            emptyContainer.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            emptyContainer.topAnchor.constraint(equalTo: container.topAnchor),
            emptyContainer.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            emptyStack.leadingAnchor.constraint(
                equalTo: emptyContainer.leadingAnchor,
                constant: AppMetrics.editorInsetH + 12
            ),
            emptyStack.widthAnchor.constraint(equalToConstant: 360),
            emptyStack.centerYAnchor.constraint(
                equalTo: emptyContainer.centerYAnchor, constant: -20
            ),
        ])

        return container
    }

    // MARK: Status bar

    private func makeStatusBar() -> NSView {
        let bar = NSView()
        bar.wantsLayer = true
        bar.layer?.backgroundColor = AppPalette.sidebarBg.cgColor

        let topBorder = NSView()
        topBorder.wantsLayer = true
        topBorder.layer?.backgroundColor = AppPalette.border.cgColor
        topBorder.translatesAutoresizingMaskIntoConstraints = false
        bar.addSubview(topBorder)

        statusLabel.font = NSFont.systemFont(ofSize: 11, weight: .regular)
        statusLabel.textColor = AppPalette.textMuted
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        bar.addSubview(statusLabel)

        statusMetaLabel.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        statusMetaLabel.textColor = AppPalette.textMuted
        statusMetaLabel.translatesAutoresizingMaskIntoConstraints = false
        bar.addSubview(statusMetaLabel)

        modeControl.controlSize = .mini
        modeControl.segmentStyle = .rounded
        modeControl.selectedSegment = 0
        modeControl.target = self
        modeControl.action = #selector(changeMode(_:))
        modeControl.translatesAutoresizingMaskIntoConstraints = false
        bar.addSubview(modeControl)

        NSLayoutConstraint.activate([
            topBorder.leadingAnchor.constraint(equalTo: bar.leadingAnchor),
            topBorder.trailingAnchor.constraint(equalTo: bar.trailingAnchor),
            topBorder.topAnchor.constraint(equalTo: bar.topAnchor),
            topBorder.heightAnchor.constraint(equalToConstant: 1),
            statusLabel.leadingAnchor.constraint(equalTo: bar.leadingAnchor, constant: 12),
            statusLabel.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
            modeControl.trailingAnchor.constraint(equalTo: bar.trailingAnchor, constant: -8),
            modeControl.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
            statusMetaLabel.trailingAnchor.constraint(
                equalTo: modeControl.leadingAnchor, constant: -12
            ),
            statusMetaLabel.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
            statusLabel.trailingAnchor.constraint(
                lessThanOrEqualTo: statusMetaLabel.leadingAnchor, constant: -16
            ),
        ])

        return bar
    }

    // MARK: Tab strip

    @objc private func selectTabFromStrip(_ sender: NSButton) {
        let index = sender.tag
        guard documents.indices.contains(index) else { return }
        selectedDocumentID = documents[index].id
        syncSidebarSelection()
        updateVisibleDocument()
    }

    @objc private func closeTabFromStrip(_ sender: NSButton) {
        let index = sender.tag
        guard documents.indices.contains(index) else { return }

        if documents[index].isDirty {
            let alert = NSAlert()
            alert.messageText = "Save \"\(documents[index].title)\"?"
            alert.informativeText = "Your changes will be lost if you don't save."
            alert.addButton(withTitle: "Save")
            alert.addButton(withTitle: "Don't Save")
            alert.addButton(withTitle: "Cancel")
            alert.alertStyle = .warning
            alert.beginSheetModal(for: view.window!) { [weak self] response in
                guard let self else { return }
                switch response {
                case .alertFirstButtonReturn:  // Save
                    self.saveDocument(nil)
                    self.closeDocument(at: index)
                case .alertSecondButtonReturn: // Don't Save
                    self.closeDocument(at: index)
                default:
                    break
                }
            }
        } else {
            closeDocument(at: index)
        }
    }

    @objc private func createTabFromStrip(_ sender: Any?) {
        newDocument(sender)
    }

    private func closeDocument(at index: Int) {
        let wasSelected = documents[index].id == selectedDocumentID
        documents.remove(at: index)

        if wasSelected {
            if documents.isEmpty {
                selectedDocumentID = nil
            } else {
                let newIndex = max(0, min(index, documents.count - 1))
                selectedDocumentID = documents[newIndex].id
            }
        }

        refreshAll()
    }

    private func refreshTabStrip() {
        tabStripStack.arrangedSubviews.forEach {
            tabStripStack.removeArrangedSubview($0)
            $0.removeFromSuperview()
        }

        for (index, doc) in documents.enumerated() {
            tabStripStack.addArrangedSubview(
                makeTabItem(title: doc.displayTitle, index: index,
                            isActive: doc.id == selectedDocumentID)
            )
        }

        let addBtn = NSButton(title: "+", target: self, action: #selector(createTabFromStrip(_:)))
        addBtn.isBordered = false
        addBtn.font = NSFont.systemFont(ofSize: 16, weight: .light)
        addBtn.contentTintColor = AppPalette.textMuted
        addBtn.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            addBtn.widthAnchor.constraint(equalToConstant: 28),
            addBtn.heightAnchor.constraint(equalToConstant: AppMetrics.tabBarHeight - 1),
        ])
        tabStripStack.addArrangedSubview(addBtn)
    }

    private func makeTabItem(title: String, index: Int, isActive: Bool) -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.layer?.backgroundColor = (
            isActive ? AppPalette.tabActiveBg : AppPalette.tabBarBg
        ).cgColor
        container.translatesAutoresizingMaskIntoConstraints = false

        let titleBtn = NSButton(title: title, target: self,
                                action: #selector(selectTabFromStrip(_:)))
        titleBtn.tag = index
        titleBtn.isBordered = false
        titleBtn.font = NSFont.systemFont(ofSize: 12, weight: isActive ? .medium : .regular)
        titleBtn.contentTintColor = isActive ? AppPalette.textPrimary : AppPalette.textMuted
        titleBtn.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(titleBtn)

        let closeBtn = NSButton(title: "×", target: self,
                                action: #selector(closeTabFromStrip(_:)))
        closeBtn.tag = index
        closeBtn.isBordered = false
        closeBtn.font = NSFont.systemFont(ofSize: 12, weight: .regular)
        closeBtn.contentTintColor = isActive ? AppPalette.textMuted : AppPalette.textMuted
            .withAlphaComponent(0.4)
        closeBtn.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(closeBtn)

        // Active underline
        let indicator = NSView()
        indicator.wantsLayer = true
        indicator.layer?.backgroundColor = isActive
            ? AppPalette.accent.cgColor
            : NSColor.clear.cgColor
        indicator.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(indicator)

        NSLayoutConstraint.activate([
            titleBtn.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 10),
            titleBtn.centerYAnchor.constraint(equalTo: container.centerYAnchor, constant: -1),
            closeBtn.leadingAnchor.constraint(equalTo: titleBtn.trailingAnchor, constant: 2),
            closeBtn.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -6),
            closeBtn.centerYAnchor.constraint(equalTo: container.centerYAnchor, constant: -1),
            closeBtn.widthAnchor.constraint(equalToConstant: 16),
            indicator.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            indicator.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            indicator.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            indicator.heightAnchor.constraint(equalToConstant: 2),
            container.heightAnchor.constraint(equalToConstant: AppMetrics.tabBarHeight - 1),
            container.widthAnchor.constraint(greaterThanOrEqualToConstant: AppMetrics.tabMinWidth),
        ])

        return container
    }

    // MARK: Sidebar data

    private func rebuildSidebarRows() {
        var rows: [SidebarRow] = []

        rows.append(.header("OPEN"))
        for i in documents.indices { rows.append(.openDoc(i)) }

        let openURLs = Set(documents.compactMap(\.url))
        let recent = NSDocumentController.shared.recentDocumentURLs
            .filter { !openURLs.contains($0) }
            .prefix(8)

        if !recent.isEmpty {
            rows.append(.header("RECENT"))
            recent.forEach { rows.append(.recent($0)) }
        }

        sidebarRows = rows
        sidebarTable.reloadData()
        syncSidebarSelection()
    }

    private func syncSidebarSelection() {
        guard let id = selectedDocumentID else {
            sidebarTable.deselectAll(nil)
            return
        }
        let row = sidebarRows.firstIndex {
            if case .openDoc(let i) = $0,
               documents.indices.contains(i),
               documents[i].id == id { return true }
            return false
        }
        if let row {
            sidebarTable.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
        }
    }

    // MARK: Session save / restore

    func saveSession() {
        let saved = documents.map { doc -> SavedDocument in
            // For dirty docs without a saved URL, embed the text directly.
            // For clean saved docs, omit the text (re-read from disk on restore).
            let embedText = doc.isDirty || doc.url == nil
            return SavedDocument(
                id: doc.id,
                urlPath: doc.url?.path(percentEncoded: false),
                title: doc.title,
                text: embedText ? doc.text : nil,
                isDirty: doc.isDirty
            )
        }
        SessionStore.save(SavedSession(
            documents: saved,
            selectedDocumentID: selectedDocumentID
        ))
    }

    private func restoreSessionOrSeed() {
        guard let session = SessionStore.load(), !session.documents.isEmpty else {
            seedInitialDocumentIfNeeded()
            return
        }

        var restored: [EditorDocument] = []
        for saved in session.documents {
            // Check for a recovery file first (crash recovery path)
            let recoveryText = SessionStore.loadRecovery(id: saved.id)

            if let path = saved.urlPath {
                let url = URL(fileURLWithPath: path)
                // Try to re-read from disk for clean documents
                if !saved.isDirty, let text = try? String(contentsOf: url, encoding: .utf8) {
                    restored.append(EditorDocument(
                        id: saved.id, url: url, title: saved.title,
                        text: text, isDirty: false
                    ))
                    continue
                }
                // Fall back to embedded session text or recovery text
                let text = recoveryText ?? saved.text ?? ""
                restored.append(EditorDocument(
                    id: saved.id, url: url, title: saved.title,
                    text: text, isDirty: !text.isEmpty || saved.isDirty
                ))
            } else {
                // Untitled document — restore from session text or recovery
                let text = recoveryText ?? saved.text ?? ""
                restored.append(EditorDocument(
                    id: saved.id, url: nil, title: saved.title,
                    text: text, isDirty: text.isEmpty ? false : saved.isDirty
                ))
            }
        }

        if restored.isEmpty {
            seedInitialDocumentIfNeeded()
            return
        }

        documents = restored
        selectedDocumentID = session.selectedDocumentID
            .flatMap { id in restored.first(where: { $0.id == id })?.id }
            ?? restored.first?.id
        refreshAll()
    }

    @objc private func appWillResignActive() {
        saveSession()
    }

    // MARK: Recovery

    private func scheduleRecoveryWrite(for doc: EditorDocument) {
        recoveryTimer?.invalidate()
        recoveryTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: false) { [weak self] _ in
            guard let self, let current = self.documents.first(where: { $0.id == doc.id }) else {
                return
            }
            if current.isDirty {
                SessionStore.writeRecovery(id: current.id, text: current.text)
            }
        }
    }

    // MARK: Document management

    private var selectedDocumentIndex: Int? {
        guard let id = selectedDocumentID else { return nil }
        return documents.firstIndex { $0.id == id }
    }

    private func seedInitialDocumentIfNeeded() {
        guard documents.isEmpty else { return }
        let doc = EditorDocument.untitled()
        documents = [doc]
        selectedDocumentID = doc.id
        refreshAll()
    }

    /// Rebuild sidebar + tab strip + visible document in one call.
    private func refreshAll() {
        rebuildSidebarRows()
        refreshTabStrip()
        updateVisibleDocument()
    }

    private func updateVisibleDocument() {
        guard let index = selectedDocumentIndex else {
            editorTextView.string = ""
            emptyContainer.isHidden = true
            editorScrollView.isHidden = false
            statusLabel.stringValue = "No document"
            statusMetaLabel.stringValue = ""
            modeControl.selectedSegment = mode.rawValue
            return
        }

        isSwitchingDocuments = true
        let doc = documents[index]
        applyDocumentContent(doc)
        statusLabel.stringValue = doc.url?.path(percentEncoded: false) ?? "Untitled draft"
        statusMetaLabel.stringValue = statusSummary(for: doc)
        updateWindowTitle()
        isSwitchingDocuments = false
    }

    private func updateWindowTitle() {
        if let index = selectedDocumentIndex {
            view.window?.title = documents[index].displayTitle
        }
    }

    private func updateStatusBar() {
        guard let index = selectedDocumentIndex else { return }
        let doc = documents[index]
        statusLabel.stringValue = doc.url?.path(percentEncoded: false) ?? "Untitled draft"
        statusMetaLabel.stringValue = statusSummary(for: doc)
    }

    private func openDocuments(_ urls: [URL]) {
        for url in urls {
            do {
                let text = try String(contentsOf: url, encoding: .utf8)
                if let i = documents.firstIndex(where: { $0.url == url }) {
                    documents[i].text = text
                    documents[i].isDirty = false
                    selectedDocumentID = documents[i].id
                } else {
                    let doc = EditorDocument(
                        id: UUID(), url: url, title: url.lastPathComponent,
                        text: text, isDirty: false
                    )
                    documents.append(doc)
                    selectedDocumentID = doc.id
                }
                NSDocumentController.shared.noteNewRecentDocumentURL(url)
            } catch {
                statusLabel.stringValue = "Failed to open: \(error.localizedDescription)"
            }
        }
        refreshAll()
    }

    private func writeDocument(at index: Int, to url: URL) {
        do {
            try documents[index].text.write(to: url, atomically: true, encoding: .utf8)
            let docID = documents[index].id
            documents[index].url = url
            documents[index].title = url.lastPathComponent
            documents[index].isDirty = false
            statusLabel.stringValue = "Saved \(url.lastPathComponent)"
            NSDocumentController.shared.noteNewRecentDocumentURL(url)
            SessionStore.clearRecovery(id: docID)
            saveSession()
            refreshAll()
        } catch {
            statusLabel.stringValue = "Save failed: \(error.localizedDescription)"
        }
    }

    private func suggestedFilename(for doc: EditorDocument) -> String {
        doc.url?.lastPathComponent ?? (doc.title == "Untitled" ? "Untitled.md" : doc.title)
    }

    private func applyDocumentContent(_ doc: EditorDocument) {
        let showEmpty = doc.url == nil && doc.text.isEmpty && mode == .source
        emptyContainer.isHidden = !showEmpty
        editorScrollView.isHidden = showEmpty

        switch mode {
        case .source:
            editorTextView.isEditable = true
            editorTextView.isSelectable = true
            editorTextView.string = doc.text
            editorTextView.font = NSFont.monospacedSystemFont(
                ofSize: AppMetrics.monoFontSize, weight: .regular
            )
            editorTextView.textColor = AppPalette.textPrimary
            editorTextView.textContainerInset = NSSize(
                width: AppMetrics.editorInsetH, height: AppMetrics.editorInsetV
            )
            editorTextView.isHorizontallyResizable = false
            editorTextView.textContainer?.widthTracksTextView = true
        case .preview:
            editorTextView.isEditable = false
            editorTextView.isSelectable = true
            editorTextView.textContainerInset = NSSize(
                width: AppMetrics.editorInsetH + 24, height: AppMetrics.editorInsetV
            )
            editorTextView.isHorizontallyResizable = false
            editorTextView.textContainer?.widthTracksTextView = true
            editorTextView.textStorage?.setAttributedString(renderPreview(for: doc))
        }

        modeControl.selectedSegment = mode.rawValue
    }

    private func renderPreview(for doc: EditorDocument) -> NSAttributedString {
        let para = NSMutableParagraphStyle()
        para.lineHeightMultiple = AppMetrics.lineHeightMultiple
        para.paragraphSpacing = 10
        para.paragraphSpacingBefore = 2

        if isMarkdown(doc), let attributed = try? AttributedString(markdown: doc.text) {
            let rendered = NSMutableAttributedString(
                attributedString: NSAttributedString(attributed)
            )
            let range = NSRange(location: 0, length: rendered.length)
            rendered.addAttribute(.paragraphStyle, value: para, range: range)
            rendered.addAttribute(.foregroundColor, value: AppPalette.textPrimary, range: range)
            return rendered
        }

        return NSAttributedString(string: doc.text, attributes: [
            .font: NSFont.systemFont(ofSize: 15, weight: .regular),
            .foregroundColor: AppPalette.textPrimary,
            .paragraphStyle: para,
        ])
    }

    private func isMarkdown(_ doc: EditorDocument) -> Bool {
        guard let ext = doc.url?.pathExtension.lowercased() else { return true }
        return ["md", "markdown", "mdown", "txt"].contains(ext)
    }

    private func statusSummary(for doc: EditorDocument) -> String {
        let modeStr  = mode == .source ? "Source" : "Preview"
        let kindStr  = isMarkdown(doc) ? "Markdown" : "Text"
        let dirtyStr = doc.isDirty ? "Unsaved" : "Saved"
        return "\(modeStr) · \(kindStr) · \(dirtyStr)"
    }
}

// MARK: - FlatSplitView

private final class FlatSplitView: NSSplitView {
    override var dividerColor: NSColor { AppPalette.border }
}

// MARK: - Row views

/// Selection row: accent at 15% opacity.
private final class FlatTableRowView: NSTableRowView {
    override func drawSelection(in dirtyRect: NSRect) {
        AppPalette.accent.withAlphaComponent(0.15).setFill()
        NSBezierPath.fill(bounds)
    }
    override var isEmphasized: Bool { get { false } set {} }
}

/// Header row: no selection highlight.
private final class TransparentTableRowView: NSTableRowView {
    override func drawSelection(in dirtyRect: NSRect) {}
    override var isEmphasized: Bool { get { false } set {} }
}
