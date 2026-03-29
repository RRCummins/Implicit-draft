// ViewController.swift
// Single-window flat layout for Implicit Standalone.
// Phase 1: design system wired in, header removed, editor card/margins gone.

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

// MARK: - ViewController

final class ViewController: NSViewController, NSTableViewDataSource, NSTableViewDelegate,
                             NSTextViewDelegate, NSMenuItemValidation {
    // MARK: Subviews
    private let splitView          = FlatSplitView()
    private let documentsTableView = NSTableView(frame: .zero)
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
    private var isSwitchingDocuments: Bool = false
    private var mode:                 EditorMode = .source
    private var didSetInitialSplit:   Bool = false
    private var theme:                ImplicitTheme = ImplicitThemeLoader.load()

    // MARK: Lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()
        buildInterface()
        seedInitialDocumentIfNeeded()
        updateVisibleDocument()
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
        documentsTableView.reloadData()
        documentsTableView.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        updateVisibleDocument()
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
            updateVisibleDocument()
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

    func numberOfRows(in tableView: NSTableView) -> Int { documents.count }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        let cellID = NSUserInterfaceItemIdentifier("DocRow")
        let cell: NSTableCellView
        if let existing = tableView.makeView(withIdentifier: cellID, owner: self) as? NSTableCellView {
            cell = existing
        } else {
            cell = NSTableCellView()
            cell.identifier = cellID
            let tf = NSTextField(labelWithString: "")
            tf.identifier = NSUserInterfaceItemIdentifier("DocLabel")
            tf.font = NSFont.systemFont(ofSize: AppMetrics.bodyFontSize, weight: .regular)
            tf.lineBreakMode = .byTruncatingMiddle
            tf.translatesAutoresizingMaskIntoConstraints = false
            cell.addSubview(tf)
            NSLayoutConstraint.activate([
                tf.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 12),
                tf.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -8),
                tf.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            ])
        }
        if let tf = cell.subviews.compactMap({ $0 as? NSTextField }).first {
            let isSelected = documents[row].id == selectedDocumentID
            tf.stringValue = documents[row].displayTitle
            tf.textColor = isSelected ? AppPalette.textPrimary : AppPalette.textMuted
        }
        return cell
    }

    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        FlatTableRowView()
    }

    // MARK: NSTableViewDelegate

    func tableViewSelectionDidChange(_ notification: Notification) {
        let row = documentsTableView.selectedRow
        guard documents.indices.contains(row) else { return }
        selectedDocumentID = documents[row].id
        updateVisibleDocument()
    }

    // MARK: NSTextViewDelegate

    func textDidChange(_ notification: Notification) {
        guard !isSwitchingDocuments, let index = selectedDocumentIndex else { return }
        documents[index].text = editorTextView.string
        documents[index].isDirty = true
        updateWindowTitle()
        documentsTableView.reloadData()
        refreshTabStrip()
        updateStatusBar()
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

    // MARK: Body (sidebar + editor)

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

        let sectionHeader = NSTextField(labelWithString: "DOCUMENTS")
        sectionHeader.font = NSFont.systemFont(ofSize: 10, weight: .semibold)
        sectionHeader.textColor = AppPalette.textMuted
        sectionHeader.translatesAutoresizingMaskIntoConstraints = false
        sidebar.addSubview(sectionHeader)

        documentsTableView.headerView = nil
        documentsTableView.style = .plain
        documentsTableView.rowHeight = AppMetrics.sidebarRowHeight
        documentsTableView.focusRingType = .none
        documentsTableView.backgroundColor = .clear
        documentsTableView.intercellSpacing = .zero
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("documents"))
        documentsTableView.addTableColumn(column)
        documentsTableView.delegate = self
        documentsTableView.dataSource = self

        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = documentsTableView
        sidebar.addSubview(scroll)

        // Right-edge border (avoids relying on NSSplitView's divider color)
        let rightBorder = NSView()
        rightBorder.wantsLayer = true
        rightBorder.layer?.backgroundColor = AppPalette.border.cgColor
        rightBorder.translatesAutoresizingMaskIntoConstraints = false
        sidebar.addSubview(rightBorder)

        NSLayoutConstraint.activate([
            sectionHeader.leadingAnchor.constraint(equalTo: sidebar.leadingAnchor, constant: 12),
            sectionHeader.trailingAnchor.constraint(equalTo: sidebar.trailingAnchor, constant: -4),
            sectionHeader.topAnchor.constraint(equalTo: sidebar.topAnchor, constant: 8),
            sectionHeader.heightAnchor.constraint(equalToConstant: 28),
            scroll.leadingAnchor.constraint(equalTo: sidebar.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: rightBorder.leadingAnchor),
            scroll.topAnchor.constraint(equalTo: sectionHeader.bottomAnchor),
            scroll.bottomAnchor.constraint(equalTo: sidebar.bottomAnchor),
            rightBorder.trailingAnchor.constraint(equalTo: sidebar.trailingAnchor),
            rightBorder.topAnchor.constraint(equalTo: sidebar.topAnchor),
            rightBorder.bottomAnchor.constraint(equalTo: sidebar.bottomAnchor),
            rightBorder.widthAnchor.constraint(equalToConstant: 1),
        ])

        return sidebar
    }

    // MARK: Editor

    private func makeEditor() -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.layer?.backgroundColor = AppPalette.windowBg.cgColor

        // Editor scroll + text view
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
        editorTextView.font = NSFont.monospacedSystemFont(ofSize: AppMetrics.monoFontSize, weight: .regular)
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

        // Empty state — inline centered content, no card border
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
        emptyBodyLabel.maximumNumberOfLines = 0
        emptyBodyLabel.lineBreakMode = .byWordWrapping

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
            emptyStack.widthAnchor.constraint(equalToConstant: 380),
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
        modeControl.segmentStyle = .texturedSquare
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
            statusMetaLabel.trailingAnchor.constraint(equalTo: modeControl.leadingAnchor, constant: -12),
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
        documentsTableView.selectRowIndexes(IndexSet(integer: index), byExtendingSelection: false)
        updateVisibleDocument()
    }

    @objc private func createTabFromStrip(_ sender: Any?) {
        newDocument(sender)
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

        let btn = NSButton(title: title, target: self, action: #selector(selectTabFromStrip(_:)))
        btn.tag = index
        btn.isBordered = false
        btn.font = NSFont.systemFont(ofSize: 12, weight: isActive ? .medium : .regular)
        btn.contentTintColor = isActive ? AppPalette.textPrimary : AppPalette.textMuted
        btn.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(btn)

        // Active underline
        let indicator = NSView()
        indicator.wantsLayer = true
        indicator.layer?.backgroundColor = isActive
            ? AppPalette.accent.cgColor
            : NSColor.clear.cgColor
        indicator.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(indicator)

        NSLayoutConstraint.activate([
            btn.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 12),
            btn.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -12),
            btn.topAnchor.constraint(equalTo: container.topAnchor),
            btn.bottomAnchor.constraint(equalTo: indicator.topAnchor),
            indicator.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            indicator.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            indicator.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            indicator.heightAnchor.constraint(equalToConstant: 2),
            container.heightAnchor.constraint(equalToConstant: AppMetrics.tabBarHeight - 1),
            container.widthAnchor.constraint(greaterThanOrEqualToConstant: AppMetrics.tabMinWidth),
        ])

        return container
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
        documentsTableView.reloadData()
        documentsTableView.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        refreshTabStrip()
    }

    private func updateVisibleDocument() {
        guard let index = selectedDocumentIndex else {
            editorTextView.string = ""
            emptyContainer.isHidden = true
            editorScrollView.isHidden = false
            statusLabel.stringValue = "No document"
            statusMetaLabel.stringValue = ""
            refreshTabStrip()
            return
        }

        isSwitchingDocuments = true
        let doc = documents[index]
        applyDocumentContent(doc)
        statusLabel.stringValue = doc.url?.path(percentEncoded: false) ?? "Untitled draft"
        statusMetaLabel.stringValue = statusSummary(for: doc)
        updateWindowTitle()
        refreshTabStrip()
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
        documentsTableView.reloadData()
        if let index = selectedDocumentIndex {
            documentsTableView.selectRowIndexes(IndexSet(integer: index), byExtendingSelection: false)
        }
        updateVisibleDocument()
    }

    private func writeDocument(at index: Int, to url: URL) {
        do {
            try documents[index].text.write(to: url, atomically: true, encoding: .utf8)
            documents[index].url = url
            documents[index].title = url.lastPathComponent
            documents[index].isDirty = false
            statusLabel.stringValue = "Saved \(url.lastPathComponent)"
            NSDocumentController.shared.noteNewRecentDocumentURL(url)
            documentsTableView.reloadData()
            updateVisibleDocument()
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

// MARK: - FlatTableRowView

private final class FlatTableRowView: NSTableRowView {
    override func drawSelection(in dirtyRect: NSRect) {
        AppPalette.accent.withAlphaComponent(0.15).setFill()
        NSBezierPath.fill(bounds)
    }

    // Prevent AppKit from overriding our draw with its own emphasis style.
    override var isEmphasized: Bool {
        get { false }
        set {}
    }
}
