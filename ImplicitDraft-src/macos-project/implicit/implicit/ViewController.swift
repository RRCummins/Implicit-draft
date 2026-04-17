// ViewController.swift
// Single-window flat layout — Phase 2: session persistence.

import Cocoa
import WebKit

// Unified sidebar row model. Section header rows are not selectable.
private enum SidebarRow {
    case header(String)
    case openDoc(Int)    // index into documents
    case recent(URL)
}

// MARK: - ViewController

final class ViewController: NSViewController,
                             NSTableViewDataSource, NSTableViewDelegate,
                             NSTextViewDelegate, NSMenuItemValidation,
                             NSWindowDelegate, WKNavigationDelegate {
    // MARK: Subviews
    private let splitView          = FlatSplitView()
    private let sidebarTable       = NSTableView(frame: .zero)
    private let editorTextView     = NSTextView(frame: .zero)
    private let editorScrollView   = NSScrollView()
    private let previewWebView     = WKWebView(frame: .zero)
    private let tabStripStack      = NSStackView()
    private let statusLabel        = NSTextField(labelWithString: "Untitled draft")
    private let statusMetaLabel    = NSTextField(labelWithString: "Edit · Markdown · Saved")
    private let sidebarTitleLabel  = NSTextField(labelWithString: "Documents")
    private let sidebarMetaLabel   = NSTextField(labelWithString: "0 open")
    private let modeControl        = NSSegmentedControl(
        labels: ["Edit", "Preview"], trackingMode: .selectOne, target: nil, action: nil
    )
    private let emptyContainer     = NSView()
    private let emptyTitleLabel    = NSTextField(labelWithString: "Start a draft")
    private let emptyBodyLabel     = NSTextField(
        labelWithString: "Write immediately, or open an existing file."
    )
    private let emptyOpenButton    = NSButton(title: "Open File →", target: nil, action: nil)

    // MARK: State
    private let session = StandaloneSession()
    private var sidebarRows:          [SidebarRow] = []
    private var isSwitchingDocuments: Bool = false
    private var mode:                 EditorMode = .source
    private var didSetInitialSplit:   Bool = false
    private var theme:                ImplicitTheme = ImplicitThemeLoader.load()
    private var recoveryTimer:        Timer?
    private var pendingWindowCloseApproval = false
    private var pendingPreviewScrollOffset: Double?

    // MARK: Public interface for AppDelegate

    var hasDirtyDocuments: Bool { session.hasDirtyDocuments }
    var dirtyDocumentCount: Int { session.dirtyDocumentCount }

    // MARK: Lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()
        buildInterface()
        session.restoreOrSeed()
        refreshAll()
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
        window.delegate = self
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
    func confirmTermination(_ completion: @escaping (Bool) -> Void) {
        confirmLossyClose(for: dirtyDocumentIndices.reversed(), completion: completion)
    }

    override var representedObject: Any? { didSet {} }

    // MARK: Actions

    @IBAction func newDocument(_ sender: Any?) {
        captureCurrentDocumentViewState()
        _ = session.createUntitled(atStart: true)
        refreshAll()
        saveSession()
    }

    @IBAction func openDocument(_ sender: Any?) {
        guard let window = view.window else { return }
        StandaloneDocumentCoordinator.requestOpenURLs(window: window) { [weak self] urls in
            guard !urls.isEmpty else { return }
            self?.openDocuments(urls)
        }
    }

    @IBAction func saveDocument(_ sender: Any?) {
        guard let index = selectedDocumentIndex else { return }
        saveDocument(at: index, completion: nil)
    }

    @IBAction func saveDocumentAs(_ sender: Any?) {
        guard let index = selectedDocumentIndex else { return }
        saveDocumentAs(at: index, completion: nil)
    }

    private func saveDocument(at index: Int, completion: (() -> Void)?) {
        guard documents.indices.contains(index) else { return }
        if let conflict = StandaloneDocumentIO.saveConflictURL(for: documents[index]) {
            presentExternalChangeAlert(for: index, url: conflict, completion: completion)
            return
        }
        if let url = documents[index].url {
            writeDocument(at: index, to: url, completion: completion)
        } else {
            saveDocumentAs(at: index, completion: completion)
        }
    }

    private func saveDocumentAs(at index: Int, completion: (() -> Void)?) {
        guard let window = view.window else { return }
        StandaloneDocumentCoordinator.requestSaveURL(
            window: window,
            suggestedFilename: StandaloneDocumentIO.suggestedFilename(for: documents[index])
        ) { [weak self] url in
            guard let self, let url else { return }
            self.writeDocument(at: index, to: url, completion: completion)
        }
    }

    @IBAction func revertDocumentToSaved(_ sender: Any?) {
        guard let index = selectedDocumentIndex, documents[index].url != nil else { return }
        do {
            try StandaloneDocumentIO.revertDocument(&documents[index])
            statusLabel.stringValue = "Reverted \(documents[index].title)"
            refreshAll()
            saveSession()
        } catch {
            statusLabel.stringValue = "Error: \(error.localizedDescription)"
        }
    }

    @IBAction func changeMode(_ sender: Any?) {
        mode = EditorMode(rawValue: modeControl.selectedSegment) ?? .source
        if let index = selectedDocumentIndex {
            documents[index].mode = mode
        }
        updateVisibleDocument()
        saveSession()
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

    // MARK: NSWindowDelegate

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        if pendingWindowCloseApproval {
            pendingWindowCloseApproval = false
            return true
        }

        let dirtyIndices = Array(dirtyDocumentIndices.reversed())
        guard !dirtyIndices.isEmpty else {
            saveSession()
            return true
        }

        confirmLossyClose(for: dirtyIndices) { [weak self, weak sender] allowed in
            guard allowed, let self, let sender else { return }
            self.pendingWindowCloseApproval = true
            sender.performClose(nil)
        }
        return false
    }

    func windowWillClose(_ notification: Notification) {
        saveSession()
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
            return makeSidebarDocCell(doc: doc, isActive: isActive)
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
            activateDocument(at: index)
        case .recent(let url):
            captureCurrentDocumentViewState()
            openDocuments([url])
        }
    }

    // MARK: NSTextViewDelegate

    func textDidChange(_ notification: Notification) {
        guard !isSwitchingDocuments, let index = selectedDocumentIndex else { return }
        documents[index].text = editorTextView.string
        documents[index].isDirty = true
        captureCurrentDocumentViewState()
        updateWindowTitle()
        rebuildSidebarRows()
        refreshTabStrip()
        updateStatusBar()
        scheduleRecoveryWrite(for: documents[index])
    }

    func textViewDidChangeSelection(_ notification: Notification) {
        guard !isSwitchingDocuments else { return }
        captureCurrentDocumentViewState()
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        guard webView == previewWebView, let offset = pendingPreviewScrollOffset else { return }
        pendingPreviewScrollOffset = nil
        let point = NSPoint(x: 0, y: max(0, offset))
        guard let scrollView = previewScrollView else { return }
        scrollView.contentView.scroll(to: point)
        scrollView.reflectScrolledClipView(scrollView.contentView)
    }

    // MARK: Interface construction

    private func buildInterface() {
        view.wantsLayer = true
        view.layer?.backgroundColor = AppPalette.windowBg.cgColor

        let tabBar    = makeTabBar()
        let body      = makeBody()
        let statusBar = makeStatusBar()

        view.addSubview(tabBar)
        view.addSubview(body)
        view.addSubview(statusBar)

        // Explicit edge-to-edge constraints — avoid NSStackView centerX default.
        NSLayoutConstraint.activate([
            tabBar.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            tabBar.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            tabBar.topAnchor.constraint(equalTo: view.topAnchor),
            tabBar.heightAnchor.constraint(equalToConstant: AppMetrics.tabBarHeight),

            statusBar.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            statusBar.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            statusBar.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            statusBar.heightAnchor.constraint(equalToConstant: AppMetrics.statusBarHeight),

            body.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            body.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            body.topAnchor.constraint(equalTo: tabBar.bottomAnchor),
            body.bottomAnchor.constraint(equalTo: statusBar.topAnchor),
        ])
    }

    private var documents: [EditorDocument] {
        get { session.documents }
        set { session.documents = newValue }
    }

    private var selectedDocumentID: UUID? {
        get { session.selectedDocumentID }
        set { session.selectedDocumentID = newValue }
    }

    private var previewScrollView: NSScrollView? {
        previewWebView.subviews.compactMap { $0 as? NSScrollView }.first
    }

    // MARK: Tab bar

    private func makeTabBar() -> NSView {
        let bar = NSView()
        bar.translatesAutoresizingMaskIntoConstraints = false
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

        modeControl.controlSize = .mini
        modeControl.segmentStyle = .rounded
        modeControl.selectedSegment = 0
        modeControl.target = self
        modeControl.action = #selector(changeMode(_:))
        modeControl.translatesAutoresizingMaskIntoConstraints = false
        bar.addSubview(modeControl)

        NSLayoutConstraint.activate([
            tabStripStack.leadingAnchor.constraint(
                equalTo: bar.leadingAnchor, constant: AppMetrics.tabBarLeadInset
            ),
            tabStripStack.trailingAnchor.constraint(
                lessThanOrEqualTo: modeControl.leadingAnchor, constant: -12
            ),
            tabStripStack.topAnchor.constraint(equalTo: bar.topAnchor),
            tabStripStack.bottomAnchor.constraint(equalTo: bar.bottomAnchor, constant: -1),
            modeControl.trailingAnchor.constraint(equalTo: bar.trailingAnchor, constant: -10),
            modeControl.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
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
        sidebar.translatesAutoresizingMaskIntoConstraints = false
        sidebar.wantsLayer = true
        sidebar.layer?.backgroundColor = AppPalette.sidebarBg.cgColor

        let header = NSView()
        header.translatesAutoresizingMaskIntoConstraints = false
        sidebar.addSubview(header)

        sidebarTitleLabel.font = NSFont.systemFont(ofSize: 13, weight: .semibold)
        sidebarTitleLabel.textColor = AppPalette.textPrimary
        sidebarTitleLabel.translatesAutoresizingMaskIntoConstraints = false
        header.addSubview(sidebarTitleLabel)

        sidebarMetaLabel.font = NSFont.systemFont(ofSize: 10, weight: .regular)
        sidebarMetaLabel.textColor = AppPalette.textMuted
        sidebarMetaLabel.translatesAutoresizingMaskIntoConstraints = false
        header.addSubview(sidebarMetaLabel)

        let newButton = makeSidebarActionButton(title: "New", action: #selector(newDocument(_:)))
        let openButton = makeSidebarActionButton(title: "Open", action: #selector(openDocument(_:)))
        header.addSubview(newButton)
        header.addSubview(openButton)

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
            sidebarTitleLabel.leadingAnchor.constraint(equalTo: header.leadingAnchor, constant: 12),
            sidebarTitleLabel.topAnchor.constraint(equalTo: header.topAnchor, constant: 10),
            sidebarMetaLabel.leadingAnchor.constraint(equalTo: header.leadingAnchor, constant: 12),
            sidebarMetaLabel.topAnchor.constraint(equalTo: sidebarTitleLabel.bottomAnchor, constant: 2),
            sidebarMetaLabel.bottomAnchor.constraint(equalTo: header.bottomAnchor, constant: -8),
            openButton.trailingAnchor.constraint(equalTo: header.trailingAnchor, constant: -12),
            openButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            newButton.trailingAnchor.constraint(equalTo: openButton.leadingAnchor, constant: -6),
            newButton.centerYAnchor.constraint(equalTo: header.centerYAnchor),
            sidebarTitleLabel.trailingAnchor.constraint(lessThanOrEqualTo: newButton.leadingAnchor, constant: -12),
            header.leadingAnchor.constraint(equalTo: sidebar.leadingAnchor),
            header.trailingAnchor.constraint(equalTo: rightBorder.leadingAnchor),
            header.topAnchor.constraint(equalTo: sidebar.topAnchor),
            scroll.leadingAnchor.constraint(equalTo: sidebar.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: rightBorder.leadingAnchor),
            scroll.topAnchor.constraint(equalTo: header.bottomAnchor),
            scroll.bottomAnchor.constraint(equalTo: sidebar.bottomAnchor),
            rightBorder.trailingAnchor.constraint(equalTo: sidebar.trailingAnchor),
            rightBorder.topAnchor.constraint(equalTo: sidebar.topAnchor),
            rightBorder.bottomAnchor.constraint(equalTo: sidebar.bottomAnchor),
            rightBorder.widthAnchor.constraint(equalToConstant: 1),
        ])

        return sidebar
    }

    private func makeSidebarActionButton(title: String, action: Selector) -> NSButton {
        let button = NSButton(title: title, target: self, action: action)
        button.isBordered = false
        button.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        button.contentTintColor = AppPalette.textMuted
        button.translatesAutoresizingMaskIntoConstraints = false
        return button
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

    private func makeSidebarDocCell(doc: EditorDocument, isActive: Bool) -> NSView {
        let id = NSUserInterfaceItemIdentifier("SidebarDoc")
        let cell = NSTableCellView()
        cell.identifier = id
        let title = NSTextField(labelWithString: doc.displayTitle)
        title.font = NSFont.systemFont(ofSize: AppMetrics.bodyFontSize, weight: isActive ? .medium : .regular)
        title.textColor = isActive ? AppPalette.textPrimary : AppPalette.textPrimary.withAlphaComponent(0.92)
        title.lineBreakMode = .byTruncatingMiddle
        title.translatesAutoresizingMaskIntoConstraints = false

        let subtitle = NSTextField(labelWithString: sidebarSubtitle(for: doc))
        subtitle.font = NSFont.systemFont(ofSize: 10, weight: .regular)
        subtitle.textColor = AppPalette.textMuted
        subtitle.lineBreakMode = .byTruncatingMiddle
        subtitle.translatesAutoresizingMaskIntoConstraints = false

        cell.addSubview(title)
        cell.addSubview(subtitle)

        if doc.isDirty {
            let dirtyDot = NSView()
            dirtyDot.wantsLayer = true
            dirtyDot.layer?.backgroundColor = AppPalette.accent.cgColor
            dirtyDot.layer?.cornerRadius = 3
            dirtyDot.translatesAutoresizingMaskIntoConstraints = false
            cell.addSubview(dirtyDot)
            NSLayoutConstraint.activate([
                dirtyDot.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -10),
                dirtyDot.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
                dirtyDot.widthAnchor.constraint(equalToConstant: 6),
                dirtyDot.heightAnchor.constraint(equalToConstant: 6),
                title.trailingAnchor.constraint(lessThanOrEqualTo: dirtyDot.leadingAnchor, constant: -8),
                subtitle.trailingAnchor.constraint(lessThanOrEqualTo: dirtyDot.leadingAnchor, constant: -8),
            ])
        } else {
            NSLayoutConstraint.activate([
                title.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -10),
                subtitle.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -10),
            ])
        }

        NSLayoutConstraint.activate([
            title.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 12),
            title.topAnchor.constraint(equalTo: cell.topAnchor, constant: 7),
            subtitle.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 12),
            subtitle.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 1),
        ])
        return cell
    }

    private func makeSidebarRecentCell(url: URL) -> NSView {
        let id = NSUserInterfaceItemIdentifier("SidebarRecent")
        let cell = NSTableCellView()
        cell.identifier = id

        let name = NSTextField(labelWithString: url.lastPathComponent)
        name.font = NSFont.systemFont(ofSize: AppMetrics.bodyFontSize, weight: .regular)
        name.textColor = AppPalette.textPrimary.withAlphaComponent(0.82)
        name.lineBreakMode = .byTruncatingMiddle
        name.translatesAutoresizingMaskIntoConstraints = false

        let dir = NSTextField(
            labelWithString: url.deletingLastPathComponent().path(percentEncoded: false)
        )
        dir.font = NSFont.systemFont(ofSize: 10, weight: .regular)
        dir.textColor = AppPalette.textMuted.withAlphaComponent(0.6)
        dir.lineBreakMode = .byTruncatingMiddle
        dir.translatesAutoresizingMaskIntoConstraints = false

        let recentBadge = NSTextField(labelWithString: "RECENT")
        recentBadge.font = NSFont.systemFont(ofSize: 9, weight: .semibold)
        recentBadge.textColor = AppPalette.textMuted.withAlphaComponent(0.75)
        recentBadge.translatesAutoresizingMaskIntoConstraints = false

        cell.addSubview(name)
        cell.addSubview(dir)
        cell.addSubview(recentBadge)
        NSLayoutConstraint.activate([
            name.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 12),
            name.trailingAnchor.constraint(lessThanOrEqualTo: recentBadge.leadingAnchor, constant: -8),
            name.topAnchor.constraint(equalTo: cell.topAnchor, constant: 7),
            dir.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 12),
            dir.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -8),
            dir.topAnchor.constraint(equalTo: name.bottomAnchor, constant: 1),
            recentBadge.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -10),
            recentBadge.topAnchor.constraint(equalTo: cell.topAnchor, constant: 8),
        ])
        return cell
    }

    private func sidebarSubtitle(for doc: EditorDocument) -> String {
        if let url = doc.url {
            return url.deletingLastPathComponent().path(percentEncoded: false)
        }
        return doc.isDirty ? "Unsaved draft · edited locally" : "Unsaved draft"
    }

    // MARK: Editor

    private func makeEditor() -> NSView {
        let container = NSView()
        container.translatesAutoresizingMaskIntoConstraints = false
        container.wantsLayer = true
        container.layer?.backgroundColor = AppPalette.windowBg.cgColor

        editorScrollView.hasVerticalScroller = true
        editorScrollView.hasHorizontalScroller = false
        editorScrollView.borderType = .noBorder
        editorScrollView.drawsBackground = false
        editorScrollView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(editorScrollView)

        previewWebView.translatesAutoresizingMaskIntoConstraints = false
        previewWebView.setValue(false, forKey: "drawsBackground")
        previewWebView.navigationDelegate = self
        previewWebView.isHidden = true
        container.addSubview(previewWebView)

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
            previewWebView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            previewWebView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            previewWebView.topAnchor.constraint(equalTo: container.topAnchor),
            previewWebView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            emptyContainer.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            emptyContainer.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            emptyContainer.topAnchor.constraint(equalTo: container.topAnchor),
            emptyContainer.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            emptyStack.widthAnchor.constraint(equalToConstant: 360),
            emptyStack.trailingAnchor.constraint(
                equalTo: emptyContainer.trailingAnchor,
                constant: -(AppMetrics.editorInsetH + 12)
            ),
            emptyStack.centerYAnchor.constraint(
                equalTo: emptyContainer.centerYAnchor, constant: -20
            ),
        ])

        return container
    }

    // MARK: Status bar

    private func makeStatusBar() -> NSView {
        let bar = NSView()
        bar.translatesAutoresizingMaskIntoConstraints = false
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

        NSLayoutConstraint.activate([
            topBorder.leadingAnchor.constraint(equalTo: bar.leadingAnchor),
            topBorder.trailingAnchor.constraint(equalTo: bar.trailingAnchor),
            topBorder.topAnchor.constraint(equalTo: bar.topAnchor),
            topBorder.heightAnchor.constraint(equalToConstant: 1),
            statusLabel.leadingAnchor.constraint(equalTo: bar.leadingAnchor, constant: 12),
            statusLabel.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
            statusMetaLabel.trailingAnchor.constraint(equalTo: bar.trailingAnchor, constant: -12),
            statusMetaLabel.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
            statusLabel.trailingAnchor.constraint(
                lessThanOrEqualTo: statusMetaLabel.leadingAnchor, constant: -16
            ),
        ])

        return bar
    }

    // MARK: Tab strip

    @objc private func selectTabFromStrip(_ sender: NSButton) {
        activateDocument(at: sender.tag)
    }

    @objc private func closeTabFromStrip(_ sender: NSButton) {
        let index = sender.tag
        guard documents.indices.contains(index) else { return }

        if documents[index].isDirty {
            guard let window = view.window else { return }
            StandaloneDocumentCoordinator.requestLossyClose(
                window: window,
                title: documents[index].title
            ) { [weak self] response in
                guard let self else { return }
                switch response {
                case .save:
                    self.saveDocument(at: index) { [weak self] in
                        self?.closeDocument(at: index)
                    }
                case .discard:
                    self.closeDocument(at: index)
                case .cancel:
                    return
                }
            }
        } else {
            closeDocument(at: index)
        }
    }

    @objc private func createTabFromStrip(_ sender: Any?) {
        newDocument(sender)
    }

    private func activateDocument(at index: Int) {
        guard documents.indices.contains(index) else { return }
        captureCurrentDocumentViewState()
        session.activateDocument(at: index)
        refreshTabStrip()
        syncSidebarSelection()
        updateVisibleDocument()
        saveSession()
    }

    private func closeDocument(at index: Int) {
        captureCurrentDocumentViewState()
        session.closeDocument(at: index)
        refreshAll()
        saveSession()
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

        rows.append(.header("OPEN · \(documents.count)"))
        for i in documents.indices { rows.append(.openDoc(i)) }

        let openURLs = Set(documents.compactMap(\.url))
        let recent = NSDocumentController.shared.recentDocumentURLs
            .filter { !openURLs.contains($0) }
            .prefix(8)

        if !recent.isEmpty {
            rows.append(.header("RECENT · \(recent.count)"))
            recent.forEach { rows.append(.recent($0)) }
        }

        sidebarMetaLabel.stringValue = documents.count == 1 ? "1 open document" : "\(documents.count) open documents"
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
        captureCurrentDocumentViewState()
        session.save()
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
        session.selectedDocumentIndex()
    }

    private var dirtyDocumentIndices: [Int] {
        session.dirtyDocumentIndices
    }

    private var pristineSeedDocumentIndex: Int? {
        session.pristineSeedDocumentIndex
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
            previewWebView.isHidden = true
            statusLabel.stringValue = "No document"
            statusMetaLabel.stringValue = ""
            modeControl.selectedSegment = mode.rawValue
            return
        }

        isSwitchingDocuments = true
        let doc = documents[index]
        mode = doc.mode
        applyDocumentContent(doc)
        restoreDocumentViewState(doc)
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

    private func captureCurrentDocumentViewState() {
        guard let index = selectedDocumentIndex else { return }
        documents[index].mode = mode
        if mode == .source {
            documents[index].selectionLocation = editorTextView.selectedRange().location
            documents[index].selectionLength = editorTextView.selectedRange().length
            documents[index].scrollOffset = editorScrollView.contentView.bounds.origin.y
        } else {
            documents[index].scrollOffset = Double(
                previewScrollView?.contentView.bounds.origin.y ?? 0
            )
        }
    }

    private func restoreDocumentViewState(_ doc: EditorDocument) {
        if mode == .source {
            let length = (editorTextView.string as NSString).length
            let clampedLocation = min(max(0, doc.selectionLocation), length)
            let clampedLength = min(max(0, doc.selectionLength), max(0, length - clampedLocation))
            let range = NSRange(location: clampedLocation, length: clampedLength)
            editorTextView.setSelectedRange(range)
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                self.editorScrollView.contentView.scroll(
                    to: NSPoint(x: 0, y: max(0, doc.scrollOffset))
                )
                self.editorScrollView.reflectScrolledClipView(self.editorScrollView.contentView)
            }
        } else {
            pendingPreviewScrollOffset = doc.scrollOffset
        }
    }

    private func presentExternalChangeAlert(
        for index: Int,
        url: URL,
        completion: (() -> Void)?
    ) {
        guard let window = view.window else { return }
        StandaloneDocumentCoordinator.requestConflictResolution(
            window: window,
            url: url
        ) { [weak self] response in
            guard let self else { return }
            switch response {
            case .overwrite:
                self.writeDocument(at: index, to: url, completion: completion)
            case .saveAs:
                self.saveDocumentAs(at: index, completion: completion)
            case .cancel:
                return
            }
        }
    }

    private func openDocuments(_ urls: [URL]) {
        captureCurrentDocumentViewState()
        var changed = false
        var replaceSeedIndex = pristineSeedDocumentIndex
        for url in urls {
            if let i = documents.firstIndex(where: { $0.url == url }) {
                if documents[i].isDirty {
                    statusLabel.stringValue = "\(documents[i].title) is already open with unsaved changes"
                    activateDocument(at: i)
                    continue
                }
                do {
                    try StandaloneDocumentIO.reloadDocument(&documents[i])
                    activateDocument(at: i)
                    NSDocumentController.shared.noteNewRecentDocumentURL(url)
                    changed = true
                    continue
                } catch {
                    statusLabel.stringValue = "Failed to open: \(error.localizedDescription)"
                    continue
                }
            }

            do {
                let doc = try StandaloneDocumentIO.loadDocument(from: url)
                if let seedIndex = replaceSeedIndex, documents.indices.contains(seedIndex) {
                    documents[seedIndex] = doc
                    replaceSeedIndex = nil
                } else {
                    documents.append(doc)
                }
                selectedDocumentID = doc.id
                NSDocumentController.shared.noteNewRecentDocumentURL(url)
                changed = true
            } catch {
                statusLabel.stringValue = "Failed to open: \(error.localizedDescription)"
            }
        }
        refreshAll()
        if changed {
            saveSession()
        }
    }

    private func writeDocument(at index: Int, to url: URL, completion: (() -> Void)?) {
        do {
            captureCurrentDocumentViewState()
            let docID = try StandaloneDocumentIO.writeDocument(&documents[index], to: url)
            statusLabel.stringValue = "Saved \(url.lastPathComponent)"
            NSDocumentController.shared.noteNewRecentDocumentURL(url)
            SessionStore.clearRecovery(id: docID)
            saveSession()
            refreshAll()
            completion?()
        } catch {
            statusLabel.stringValue = "Save failed: \(error.localizedDescription)"
        }
    }

    private func confirmLossyClose(
        for pendingIndices: [Int],
        completion: @escaping (Bool) -> Void
    ) {
        guard let index = pendingIndices.first, documents.indices.contains(index) else {
            completion(true)
            return
        }

        activateDocument(at: index)

        let doc = documents[index]
        guard let window = view.window else {
            completion(false)
            return
        }
        StandaloneDocumentCoordinator.requestLossyClose(
            window: window,
            title: doc.title
        ) { [weak self] response in
            guard let self else { return }
            switch response {
            case .save:
                self.saveDocument(at: index) { [weak self] in
                    guard let self else { return }
                    self.confirmLossyClose(
                        for: Array(pendingIndices.dropFirst()),
                        completion: completion
                    )
                }
            case .discard:
                self.confirmLossyClose(
                    for: Array(pendingIndices.dropFirst()),
                    completion: completion
                )
            case .cancel:
                completion(false)
            }
        }
    }

    private func applyDocumentContent(_ doc: EditorDocument) {
        let showEmpty = doc.url == nil && doc.text.isEmpty && mode == .source
        emptyContainer.isHidden = !showEmpty
        editorScrollView.isHidden = showEmpty || mode == .preview
        previewWebView.isHidden = mode != .preview

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
            previewWebView.loadHTMLString(
                renderPreviewHTML(for: doc),
                baseURL: doc.url?.deletingLastPathComponent()
            )
        }

        modeControl.selectedSegment = mode.rawValue
    }

    private func renderPreviewHTML(for doc: EditorDocument) -> String {
        let title = escapeHTML(doc.title)
        let body = isMarkdown(doc) ? markdownToHTML(doc.text) : plainTextToHTML(doc.text)
        return """
        <!doctype html>
        <html>
        <head>
          <meta charset="utf-8">
          <meta name="viewport" content="width=device-width, initial-scale=1">
          <title>\(title)</title>
          <style>
            :root {
              color-scheme: dark;
              --bg: \(cssHex(AppPalette.windowBg));
              --panel: \(cssHex(AppPalette.sidebarBg));
              --text: \(cssHex(AppPalette.textPrimary));
              --muted: \(cssHex(AppPalette.textMuted));
              --border: \(cssHex(AppPalette.border));
              --accent: \(cssHex(AppPalette.accent));
              --code: \(cssHex(AppPalette.tabActiveBg));
            }
            html, body {
              margin: 0;
              background: var(--bg);
              color: var(--text);
              font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
            }
            body { padding: 32px 0 56px; }
            main {
              max-width: 820px;
              margin: 0 auto;
              padding: 0 44px;
              box-sizing: border-box;
            }
            h1, h2, h3, h4, h5, h6 {
              line-height: 1.15;
              margin: 1.25em 0 0.5em;
            }
            h1 { font-size: 2rem; }
            h2 { font-size: 1.55rem; }
            h3 { font-size: 1.25rem; }
            p, li, blockquote {
              font-size: 15px;
              line-height: 1.7;
            }
            p, ul, ol, pre, blockquote { margin: 0 0 1rem; }
            ul, ol { padding-left: 1.4rem; }
            .task-list {
              list-style: none;
              padding-left: 0;
            }
            .task-item {
              display: flex;
              align-items: flex-start;
              gap: 10px;
              margin-bottom: 0.55rem;
            }
            .task-item input {
              margin-top: 0.28rem;
              accent-color: var(--accent);
            }
            .task-item.done span {
              color: var(--muted);
              text-decoration: line-through;
            }
            code {
              font-family: "SF Mono", Menlo, monospace;
              font-size: 0.92em;
              background: color-mix(in srgb, var(--code) 88%, transparent);
              border: 1px solid var(--border);
              border-radius: 6px;
              padding: 0.12rem 0.35rem;
            }
            pre {
              background: var(--code);
              border: 1px solid var(--border);
              border-radius: 12px;
              padding: 16px 18px;
              overflow-x: auto;
            }
            .code-block {
              margin: 0 0 1rem;
              border: 1px solid var(--border);
              border-radius: 12px;
              overflow: hidden;
              background: var(--code);
            }
            .code-block .code-label {
              display: inline-flex;
              align-items: center;
              min-height: 28px;
              padding: 0 12px;
              border-bottom: 1px solid var(--border);
              color: var(--muted);
              font-size: 11px;
              font-weight: 600;
              letter-spacing: 0.04em;
              text-transform: uppercase;
              background: color-mix(in srgb, var(--panel) 48%, transparent);
            }
            .code-block pre {
              margin: 0;
              border: 0;
              border-radius: 0;
            }
            pre code {
              background: transparent;
              border: 0;
              padding: 0;
            }
            blockquote {
              border-left: 3px solid var(--accent);
              padding-left: 14px;
              color: var(--muted);
            }
            hr {
              border: 0;
              height: 1px;
              background: var(--border);
              margin: 1.5rem 0;
            }
            a {
              color: var(--accent);
              text-decoration: none;
            }
            img {
              display: block;
              max-width: 100%;
              height: auto;
              border: 1px solid var(--border);
              border-radius: 14px;
              margin: 0 0 1rem;
            }
            .table-wrap {
              margin: 0 0 1rem;
              overflow-x: auto;
              border: 1px solid var(--border);
              border-radius: 12px;
              background: color-mix(in srgb, var(--panel) 35%, transparent);
            }
            table {
              width: 100%;
              border-collapse: separate;
              border-spacing: 0;
              font-size: 14px;
              margin: 0;
            }
            th, td {
              padding: 10px 12px;
              text-align: left;
              vertical-align: top;
              border-right: 1px solid var(--border);
              border-bottom: 1px solid var(--border);
            }
            th:last-child, td:last-child { border-right: 0; }
            tbody tr:last-child td { border-bottom: 0; }
            thead th {
              background: color-mix(in srgb, var(--panel) 78%, transparent);
              font-weight: 600;
            }
            tbody tr:nth-child(even) td {
              background: color-mix(in srgb, var(--panel) 28%, transparent);
            }
          </style>
        </head>
        <body>
          <main>
            \(body)
          </main>
        </body>
        </html>
        """
    }

    private func plainTextToHTML(_ text: String) -> String {
        "<pre><code>\(escapeHTML(text))</code></pre>"
    }

    private func markdownToHTML(_ markdown: String) -> String {
        var html: [String] = []
        var paragraph: [String] = []
        var listItems: [String] = []
        var currentListTag: String?
        var inCodeBlock = false
        var codeLines: [String] = []
        var codeFenceLanguage: String?
        let lines = markdown.components(separatedBy: .newlines)

        func flushParagraph() {
            guard !paragraph.isEmpty else { return }
            html.append("<p>\(renderInlineMarkdown(paragraph.joined(separator: " ")))</p>")
            paragraph.removeAll()
        }

        func flushList() {
            guard let tagName = currentListTag, !listItems.isEmpty else { return }
            html.append("<\(tagName)>\(listItems.joined())</\(tagName)>")
            listItems.removeAll()
            currentListTag = nil
        }

        func flushCodeBlock() {
            guard !codeLines.isEmpty else { return }
            let escapedCode = escapeHTML(codeLines.joined(separator: "\n"))
            if let language = codeFenceLanguage, !language.isEmpty {
                html.append("""
                <div class="code-block">
                  <div class="code-label">\(escapeHTML(language))</div>
                  <pre><code class="language-\(escapeHTML(language.lowercased()))">\(escapedCode)</code></pre>
                </div>
                """)
            } else {
                html.append("<pre><code>\(escapedCode)</code></pre>")
            }
            codeLines.removeAll()
            codeFenceLanguage = nil
        }

        var lineIndex = 0
        while lineIndex < lines.count {
            let rawLine = lines[lineIndex]
            let line = rawLine.trimmingCharacters(in: .whitespaces)

            if rawLine.hasPrefix("```") {
                flushParagraph()
                flushList()
                if inCodeBlock { flushCodeBlock() }
                let fenceSuffix = rawLine.dropFirst(3).trimmingCharacters(in: .whitespaces)
                codeFenceLanguage = fenceSuffix.isEmpty ? nil : fenceSuffix
                inCodeBlock.toggle()
                lineIndex += 1
                continue
            }

            if inCodeBlock {
                codeLines.append(rawLine)
                lineIndex += 1
                continue
            }

            if line.isEmpty {
                flushParagraph()
                flushList()
                lineIndex += 1
                continue
            }

            if let table = parseMarkdownTable(lines: lines, startingAt: lineIndex) {
                flushParagraph()
                flushList()
                html.append(renderTableHTML(table))
                lineIndex = table.nextIndex
                continue
            }

            if line == "---" || line == "***" {
                flushParagraph()
                flushList()
                html.append("<hr>")
                lineIndex += 1
                continue
            }

            if let heading = parseHeading(line) {
                flushParagraph()
                flushList()
                html.append("<h\(heading.level)>\(renderInlineMarkdown(heading.text))</h\(heading.level)>")
                lineIndex += 1
                continue
            }

            if let taskItem = parseTaskListItem(line) {
                flushParagraph()
                if currentListTag != "ul class=\"task-list\"" {
                    flushList()
                    currentListTag = "ul class=\"task-list\""
                }
                let checked = taskItem.completed ? " checked" : ""
                let doneClass = taskItem.completed ? " class=\"task-item done\"" : " class=\"task-item\""
                listItems.append(
                    "<li\(doneClass)><input type=\"checkbox\" disabled\(checked)><span>\(renderInlineMarkdown(taskItem.text))</span></li>"
                )
                lineIndex += 1
                continue
            }

            if let item = line.dropPrefixIfPresent("- ") ?? line.dropPrefixIfPresent("* ") {
                flushParagraph()
                if currentListTag != "ul" {
                    flushList()
                    currentListTag = "ul"
                }
                listItems.append("<li>\(renderInlineMarkdown(item))</li>")
                lineIndex += 1
                continue
            }

            if let item = line.captureOrderedListItem() {
                flushParagraph()
                if currentListTag != "ol" {
                    flushList()
                    currentListTag = "ol"
                }
                listItems.append("<li>\(renderInlineMarkdown(item))</li>")
                lineIndex += 1
                continue
            }

            if let quote = line.dropPrefixIfPresent("> ") {
                flushParagraph()
                flushList()
                html.append("<blockquote>\(renderInlineMarkdown(quote))</blockquote>")
                lineIndex += 1
                continue
            }

            flushList()
            paragraph.append(line)
            lineIndex += 1
        }

        if inCodeBlock { flushCodeBlock() }
        flushParagraph()
        flushList()
        return html.joined(separator: "\n")
    }

    private func parseMarkdownTable(lines: [String], startingAt index: Int)
        -> (header: [String], alignments: [String?], rows: [[String]], nextIndex: Int)?
    {
        guard index + 1 < lines.count,
              let header = splitMarkdownTableRow(lines[index]),
              let alignments = parseMarkdownTableSeparator(lines[index + 1]),
              header.count == alignments.count else {
            return nil
        }

        var rows: [[String]] = []
        var nextIndex = index + 2

        while nextIndex < lines.count {
            let rawLine = lines[nextIndex]
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

        return (header, alignments, rows, nextIndex)
    }

    private func splitMarkdownTableRow(_ rawLine: String) -> [String]? {
        let trimmed = rawLine.trimmingCharacters(in: .whitespaces)
        guard trimmed.contains("|") else { return nil }

        let core = trimmed.trimmingCharacters(in: CharacterSet(charactersIn: "|"))
        let parts = core.split(separator: "|", omittingEmptySubsequences: false)
            .map { $0.trimmingCharacters(in: .whitespaces) }
        return parts.isEmpty ? nil : parts
    }

    private func parseMarkdownTableSeparator(_ rawLine: String) -> [String?]? {
        guard let parts = splitMarkdownTableRow(rawLine) else { return nil }

        var alignments: [String?] = []
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
                alignments.append("center")
            } else if trailingColon {
                alignments.append("right")
            } else if leadingColon {
                alignments.append("left")
            } else {
                alignments.append(nil)
            }
        }

        return alignments
    }

    private func renderTableHTML(
        _ table: (header: [String], alignments: [String?], rows: [[String]], nextIndex: Int)
    ) -> String {
        func style(for alignment: String?) -> String {
            guard let alignment else { return "" }
            return " style=\"text-align: \(alignment);\""
        }

        let headerHTML = zip(table.header, table.alignments).map { cell, alignment in
            "<th\(style(for: alignment))>\(renderInlineMarkdown(cell))</th>"
        }.joined()

        let bodyHTML = table.rows.map { row in
            let cells = zip(row, table.alignments).map { cell, alignment in
                "<td\(style(for: alignment))>\(renderInlineMarkdown(cell))</td>"
            }.joined()
            return "<tr>\(cells)</tr>"
        }.joined()

        let bodySection = bodyHTML.isEmpty ? "" : "<tbody>\(bodyHTML)</tbody>"
        return """
        <div class="table-wrap">
          <table>
            <thead><tr>\(headerHTML)</tr></thead>
            \(bodySection)
          </table>
        </div>
        """
    }

    private func parseHeading(_ line: String) -> (level: Int, text: String)? {
        let hashes = line.prefix { $0 == "#" }
        guard (1...6).contains(hashes.count), line.dropFirst(hashes.count).hasPrefix(" ") else {
            return nil
        }
        let text = line.dropFirst(hashes.count).trimmingCharacters(in: .whitespaces)
        return (hashes.count, text)
    }

    private func parseTaskListItem(_ line: String) -> (completed: Bool, text: String)? {
        let prefixes = ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] "]
        for prefix in prefixes {
            guard line.hasPrefix(prefix) else { continue }
            let completed = prefix.lowercased().contains("[x]")
            return (completed, String(line.dropFirst(prefix.count)))
        }
        return nil
    }

    private func renderInlineMarkdown(_ text: String) -> String {
        var rendered = escapeHTML(text)
        rendered = rendered.replacingOccurrences(
            of: #"!\[([^\]]*)\]\(([^)]+)\)"#,
            with: "<img src=\"$2\" alt=\"$1\">",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"`([^`]+)`"#,
            with: "<code>$1</code>",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"\*\*([^*]+)\*\*"#,
            with: "<strong>$1</strong>",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"(?<!\*)\*([^*]+)\*(?!\*)"#,
            with: "<em>$1</em>",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"~~([^~]+)~~"#,
            with: "<del>$1</del>",
            options: .regularExpression
        )
        rendered = rendered.replacingOccurrences(
            of: #"\[([^\]]+)\]\(([^)]+)\)"#,
            with: "<a href=\"$2\">$1</a>",
            options: .regularExpression
        )
        return rendered
    }

    private func escapeHTML(_ value: String) -> String {
        value
            .replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
            .replacingOccurrences(of: "\"", with: "&quot;")
    }

    private func cssHex(_ color: NSColor) -> String {
        let rgb = color.usingColorSpace(.deviceRGB) ?? color
        let r = Int((rgb.redComponent * 255).rounded())
        let g = Int((rgb.greenComponent * 255).rounded())
        let b = Int((rgb.blueComponent * 255).rounded())
        return String(format: "#%02X%02X%02X", r, g, b)
    }

    private func isMarkdown(_ doc: EditorDocument) -> Bool {
        guard let ext = doc.url?.pathExtension.lowercased() else { return true }
        return ["md", "markdown", "mdown", "txt"].contains(ext)
    }

    private func statusSummary(for doc: EditorDocument) -> String {
        let modeStr  = mode == .source ? "Edit" : "Preview"
        let kindStr  = isMarkdown(doc) ? "Markdown" : "Text"
        let dirtyStr = doc.isDirty ? "Unsaved" : "Saved"
        return "\(modeStr) · \(kindStr) · \(dirtyStr)"
    }
}

private extension String {
    func dropPrefixIfPresent(_ prefix: String) -> String? {
        hasPrefix(prefix) ? String(dropFirst(prefix.count)) : nil
    }

    func captureOrderedListItem() -> String? {
        guard let dot = firstIndex(of: ".") else { return nil }
        let prefix = self[..<dot]
        guard !prefix.isEmpty, prefix.allSatisfy(\.isNumber) else { return nil }
        let remainderStart = index(after: dot)
        guard remainderStart < endIndex, self[remainderStart] == " " else { return nil }
        let contentStart = index(after: remainderStart)
        guard contentStart <= endIndex else { return nil }
        return String(self[contentStart...])
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
