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
        labels: ["Edit", "Live", "Preview"], trackingMode: .selectOne, target: nil, action: nil
    )
    private let emptyContainer     = NSView()
    private let emptyTitleLabel    = NSTextField(labelWithString: "Start a draft")
    private let emptyBodyLabel     = NSTextField(
        labelWithString: "Write immediately, or open an existing file."
    )
    private let emptyOpenButton    = NSButton(title: "Open File →", target: nil, action: nil)
    private let formatBar          = NSStackView()

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
    private var isApplyingEditorStyle = false
    private var liveSyntaxRevealRange: NSRange?
    private var liveSyntaxHideTimer: Timer?
    private var pendingEditorStyleWorkItem: DispatchWorkItem?

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
        mode = EditorMode.fromFooterSegmentIndex(modeControl.selectedSegment)
        if let index = selectedDocumentIndex {
            documents[index].mode = mode
        }
        if mode == .live {
            revealLiveSyntaxAroundSelection()
            scheduleLiveSyntaxHide()
        } else {
            clearLiveSyntaxReveal()
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
        guard !isSwitchingDocuments, !isApplyingEditorStyle, let index = selectedDocumentIndex else { return }
        documents[index].text = editorTextView.string
        documents[index].isDirty = true
        if mode == .live {
            revealLiveSyntaxAroundSelection()
            scheduleLiveSyntaxHide()
        }
        scheduleEditorStyleRefresh(for: documents[index])
        captureCurrentDocumentViewState()
        updateWindowTitle()
        rebuildSidebarRows()
        refreshTabStrip()
        updateStatusBar()
        scheduleRecoveryWrite(for: documents[index])
    }

    func textViewDidChangeSelection(_ notification: Notification) {
        guard !isSwitchingDocuments, !isApplyingEditorStyle else { return }
        if mode == .live, let index = selectedDocumentIndex {
            revealLiveSyntaxAroundSelection()
            scheduleEditorStyleRefresh(for: documents[index])
            scheduleLiveSyntaxHide()
        }
        captureCurrentDocumentViewState()
    }

    func textView(_ textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        guard textView == editorTextView,
              mode != .preview,
              isMarkdownCurrentDocument else {
            return false
        }

        switch commandSelector {
        case #selector(NSResponder.insertNewline(_:)):
            return handleMarkdownInsertNewline()
        case #selector(NSResponder.insertTab(_:)):
            return handleMarkdownIndent(outdent: false)
        case #selector(NSResponder.insertBacktab(_:)):
            return handleMarkdownIndent(outdent: true)
        default:
            return false
        }
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

        configureFormatBar()
        bar.addSubview(formatBar)

        NSLayoutConstraint.activate([
            tabStripStack.leadingAnchor.constraint(
                equalTo: bar.leadingAnchor, constant: AppMetrics.tabBarLeadInset
            ),
            tabStripStack.trailingAnchor.constraint(
                lessThanOrEqualTo: formatBar.leadingAnchor, constant: -12
            ),
            tabStripStack.topAnchor.constraint(equalTo: bar.topAnchor),
            tabStripStack.bottomAnchor.constraint(equalTo: bar.bottomAnchor, constant: -1),
            formatBar.trailingAnchor.constraint(equalTo: bar.trailingAnchor, constant: -10),
            formatBar.centerYAnchor.constraint(equalTo: bar.centerYAnchor),
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

    private func configureFormatBar() {
        guard formatBar.arrangedSubviews.isEmpty else { return }

        formatBar.orientation = .horizontal
        formatBar.alignment = .centerY
        formatBar.spacing = 6
        formatBar.translatesAutoresizingMaskIntoConstraints = false

        let items: [(String, Selector)] = [
            ("H1", #selector(applyHeadingFormatting(_:))),
            ("B", #selector(applyBoldFormatting(_:))),
            ("I", #selector(applyItalicFormatting(_:))),
            ("`", #selector(applyCodeFormatting(_:))),
            ("Link", #selector(applyLinkFormatting(_:))),
        ]

        for (title, action) in items {
            let button = NSButton(title: title, target: self, action: action)
            button.isBordered = false
            button.font = NSFont.systemFont(
                ofSize: title == "Link" ? 11 : 11,
                weight: title == "B" || title == "H1" ? .semibold : .medium
            )
            button.contentTintColor = AppPalette.textMuted
            button.translatesAutoresizingMaskIntoConstraints = false
            formatBar.addArrangedSubview(button)
        }
    }

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

    @objc private func applyHeadingFormatting(_ sender: Any?) {
        applyHeadingPrefix("# ")
    }

    @objc private func applyBoldFormatting(_ sender: Any?) {
        applyMarkdownWrapper(prefix: "**", suffix: "**", placeholder: "bold text")
    }

    @objc private func applyItalicFormatting(_ sender: Any?) {
        applyMarkdownWrapper(prefix: "*", suffix: "*", placeholder: "italic text")
    }

    @objc private func applyCodeFormatting(_ sender: Any?) {
        applyMarkdownWrapper(prefix: "`", suffix: "`", placeholder: "code")
    }

    @objc private func applyLinkFormatting(_ sender: Any?) {
        applyMarkdownWrapper(prefix: "[", suffix: "](https://)", placeholder: "link text")
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

    private func applyMarkdownWrapper(prefix: String, suffix: String, placeholder: String) {
        guard mode != .preview, let index = selectedDocumentIndex else { return }
        let textView = editorTextView
        let currentString = textView.string as NSString
        let selectedRange = textView.selectedRange()

        let selectedText = selectedRange.length > 0
            ? currentString.substring(with: selectedRange)
            : placeholder
        let replacement = "\(prefix)\(selectedText)\(suffix)"

        textView.textStorage?.replaceCharacters(in: selectedRange, with: replacement)

        let newLocation = selectedRange.location + prefix.count
        let newLength = selectedRange.length > 0 ? selectedText.count : placeholder.count
        textView.setSelectedRange(NSRange(location: newLocation, length: newLength))

        documents[index].text = textView.string
        documents[index].isDirty = true
        captureCurrentDocumentViewState()
        updateWindowTitle()
        rebuildSidebarRows()
        refreshTabStrip()
        updateStatusBar()
        scheduleRecoveryWrite(for: documents[index])
    }

    private func applyHeadingPrefix(_ prefix: String) {
        guard mode != .preview, let index = selectedDocumentIndex else { return }
        let textView = editorTextView
        let fullText = textView.string as NSString
        let selectedRange = textView.selectedRange()

        let lineRange = fullText.lineRange(for: selectedRange)
        let blockText = fullText.substring(with: lineRange)
        let transformed = blockText.components(separatedBy: .newlines).map { line in
            guard !line.isEmpty else { return line }
            return line.hasPrefix(prefix) ? line : "\(prefix)\(line)"
        }.joined(separator: "\n")

        textView.textStorage?.replaceCharacters(in: lineRange, with: transformed)
        textView.setSelectedRange(NSRange(location: lineRange.location, length: transformed.count))

        documents[index].text = textView.string
        documents[index].isDirty = true
        captureCurrentDocumentViewState()
        updateWindowTitle()
        rebuildSidebarRows()
        refreshTabStrip()
        updateStatusBar()
        scheduleRecoveryWrite(for: documents[index])
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
            modeControl.selectedSegment = mode.footerSegmentIndex
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
        if mode != .preview {
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
        if mode != .preview {
            let length = (editorTextView.string as NSString).length
            let clampedLocation = min(max(0, doc.selectionLocation), length)
            let clampedLength = min(max(0, doc.selectionLength), max(0, length - clampedLocation))
            let range = NSRange(location: clampedLocation, length: clampedLength)
            editorTextView.setSelectedRange(range)
            if mode == .live {
                revealLiveSyntaxAroundSelection()
                scheduleLiveSyntaxHide()
            } else {
                clearLiveSyntaxReveal()
            }
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
        let showEmpty = doc.url == nil && doc.text.isEmpty && mode != .preview
        emptyContainer.isHidden = !showEmpty
        editorScrollView.isHidden = showEmpty || mode == .preview
        previewWebView.isHidden = mode != .preview

        switch mode {
        case .source, .live:
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
            applyEditorStyleIfNeeded(for: doc)
        case .preview:
            previewWebView.loadHTMLString(
                StandaloneMarkdownPreviewRenderer.renderPreviewHTML(
                    for: doc,
                    isMarkdown: isMarkdown(doc)
                ),
                baseURL: doc.url?.deletingLastPathComponent()
            )
        }

        modeControl.selectedSegment = mode.footerSegmentIndex
    }

    private func isMarkdown(_ doc: EditorDocument) -> Bool {
        guard let ext = doc.url?.pathExtension.lowercased() else { return true }
        return ["md", "markdown", "mdown", "txt"].contains(ext)
    }

    private var isMarkdownCurrentDocument: Bool {
        guard let index = selectedDocumentIndex else { return false }
        return isMarkdown(documents[index])
    }

    private func statusSummary(for doc: EditorDocument) -> String {
        let modeStr: String
        switch mode {
        case .source:
            modeStr = "Edit"
        case .live:
            modeStr = "Live"
        case .preview:
            modeStr = "Preview"
        }
        let kindStr  = isMarkdown(doc) ? "Markdown" : "Text"
        let dirtyStr = doc.isDirty ? "Unsaved" : "Saved"
        return "\(modeStr) · \(kindStr) · \(dirtyStr)"
    }

    private func applyEditorStyleIfNeeded(for doc: EditorDocument) {
        guard mode != .preview else { return }
        isApplyingEditorStyle = true
        let presentationMode: MarkdownEditorStyler.PresentationMode =
            mode == .live
            ? .live(revealedRange: liveSyntaxRevealRange)
            : .source
        MarkdownEditorStyler.apply(
            to: editorTextView,
            text: doc.text,
            isMarkdown: isMarkdown(doc),
            mode: presentationMode
        )
        isApplyingEditorStyle = false
    }

    private func handleMarkdownInsertNewline() -> Bool {
        guard let index = selectedDocumentIndex else { return false }
        let textView = editorTextView
        let nsText = textView.string as NSString
        let selectedRange = textView.selectedRange()
        guard selectedRange.length == 0 else { return false }

        let lineRange = nsText.lineRange(for: selectedRange)
        let rawLine = nsText.substring(with: lineRange)
        let line = rawLine.trimmingCharacters(in: CharacterSet.newlines)
        let caretOffsetInLine = selectedRange.location - lineRange.location
        let prefix = markdownContinuationPrefix(for: line)

        if let fence = codeFenceContinuation(for: line, caretOffsetInLine: caretOffsetInLine) {
            replaceSelection(with: fence.text, selectedRange: fence.selection, documentIndex: index)
            return true
        }

        guard let prefix else { return false }

        if prefix.exitWhenEmpty {
            let content = String(line.dropFirst(prefix.prefix.count)).trimmingCharacters(in: .whitespaces)
            if content.isEmpty, caretOffsetInLine >= line.count {
                let contentRange = contentRangeForLineRange(lineRange, in: nsText)
                replaceCharacters(
                    in: contentRange,
                    with: "",
                    selectedRange: NSRange(location: contentRange.location, length: 0),
                    documentIndex: index
                )
                return true
            }
        }

        let insertion = "\n\(prefix.prefix)"
        let newSelection = NSRange(location: selectedRange.location + insertion.count, length: 0)
        replaceSelection(with: insertion, selectedRange: newSelection, documentIndex: index)
        return true
    }

    private func handleMarkdownIndent(outdent: Bool) -> Bool {
        guard let index = selectedDocumentIndex else { return false }
        let textView = editorTextView
        let nsText = textView.string as NSString
        let selectedRange = textView.selectedRange()
        let lineRange = nsText.lineRange(for: selectedRange)
        let block = nsText.substring(with: lineRange)

        let hasTrailingNewline = block.hasSuffix("\n")
        let core = hasTrailingNewline ? String(block.dropLast()) : block
        let lines = core.components(separatedBy: "\n")
        guard !lines.isEmpty else { return false }

        let transformed = lines.map { line -> String in
            if outdent {
                if line.hasPrefix("\t") {
                    return String(line.dropFirst())
                }
                if line.hasPrefix("    ") {
                    return String(line.dropFirst(4))
                }
                let removable = min(line.prefix { $0 == " " }.count, 4)
                return String(line.dropFirst(removable))
            }
            return "    \(line)"
        }.joined(separator: "\n") + (hasTrailingNewline ? "\n" : "")

        let firstLineIndentWidth = leadingIndentWidthOfMarkdownLine(lines.first ?? "")
        let selectionDeltaPerLine = outdent ? -4 : 4
        let affectedLineCount = max(1, lines.count)
        let selectionLocation = max(lineRange.location, selectedRange.location + (outdent ? -min(4, firstLineIndentWidth) : 4))
        let adjustedLength = max(0, selectedRange.length + selectionDeltaPerLine * affectedLineCount)

        replaceCharacters(
            in: lineRange,
            with: transformed,
            selectedRange: NSRange(location: selectionLocation, length: adjustedLength),
            documentIndex: index
        )
        return true
    }

    private func replaceSelection(
        with replacement: String,
        selectedRange: NSRange,
        documentIndex: Int
    ) {
        replaceCharacters(
            in: editorTextView.selectedRange(),
            with: replacement,
            selectedRange: selectedRange,
            documentIndex: documentIndex
        )
    }

    private func markdownContinuationPrefix(for line: String) -> (prefix: String, exitWhenEmpty: Bool)? {
        if let match = line.wholeMatch(of: /^\s*(?<fence>```[A-Za-z0-9_-]*)\s*$/) {
            return (prefix: String(match.output.fence), exitWhenEmpty: false)
        }

        if let match = line.wholeMatch(of: /^(?<indent>\s*)(?<quote>(?:>\s*)+)(?<body>.*)$/) {
            return (prefix: String(match.output.indent) + String(match.output.quote), exitWhenEmpty: true)
        }

        if let match = line.wholeMatch(of: /^(?<indent>\s*)(?<marker>[-+*]\s+\[(?: |x|X)\]\s+)(?<body>.*)$/) {
            return (prefix: String(match.output.indent) + String(match.output.marker), exitWhenEmpty: true)
        }

        if let match = line.wholeMatch(of: /^(?<indent>\s*)(?<number>\d+)\.\s+(?<body>.*)$/),
           let value = Int(match.output.number) {
            let next = value + 1
            return (prefix: String(match.output.indent) + "\(next). ", exitWhenEmpty: true)
        }

        if let match = line.wholeMatch(of: /^(?<indent>\s*)(?<marker>[-+*]\s+)(?<body>.*)$/) {
            return (prefix: String(match.output.indent) + String(match.output.marker), exitWhenEmpty: true)
        }

        return nil
    }

    private func codeFenceContinuation(for line: String, caretOffsetInLine: Int) -> (text: String, selection: NSRange)? {
        guard line.trimmingCharacters(in: .whitespaces).hasPrefix("```"),
              caretOffsetInLine == line.count else {
            return nil
        }
        let leadingWhitespace = String(line.prefix { $0 == " " || $0 == "\t" })
        let insertion = "\n\(leadingWhitespace)\n\(leadingWhitespace)```"
        let selection = NSRange(location: editorTextView.selectedRange().location + leadingWhitespace.count + 1, length: 0)
        return (insertion, selection)
    }

    private func replaceCharacters(
        in range: NSRange,
        with replacement: String,
        selectedRange: NSRange,
        documentIndex: Int
    ) {
        editorTextView.textStorage?.replaceCharacters(in: range, with: replacement)
        editorTextView.setSelectedRange(selectedRange)
        documents[documentIndex].text = editorTextView.string
        documents[documentIndex].isDirty = true
        if mode == .live {
            revealLiveSyntaxAroundSelection()
            scheduleLiveSyntaxHide()
        }
        scheduleEditorStyleRefresh(for: documents[documentIndex])
        captureCurrentDocumentViewState()
        updateWindowTitle()
        rebuildSidebarRows()
        refreshTabStrip()
        updateStatusBar()
        scheduleRecoveryWrite(for: documents[documentIndex])
    }

    private func contentRangeForLineRange(_ lineRange: NSRange, in text: NSString) -> NSRange {
        var length = lineRange.length
        if length > 0 {
            let lastIndex = lineRange.location + length - 1
            if lastIndex < text.length, text.character(at: lastIndex) == 10 {
                length -= 1
            }
        }
        return NSRange(location: lineRange.location, length: max(0, length))
    }

    private func leadingIndentWidthOfMarkdownLine(_ line: String) -> Int {
        var width = 0
        for char in line {
            switch char {
            case " ":
                width += 1
            case "\t":
                width += 4
            default:
                return width
            }
        }
        return width
    }

    private func revealLiveSyntaxAroundSelection() {
        guard mode == .live else {
            clearLiveSyntaxReveal()
            return
        }
        let nsText = editorTextView.string as NSString
        let selectedRange = editorTextView.selectedRange()
        let safeLocation = min(max(0, selectedRange.location), nsText.length)
        let safeLength = min(max(0, selectedRange.length), max(0, nsText.length - safeLocation))
        let normalizedRange = NSRange(location: safeLocation, length: safeLength)
        liveSyntaxRevealRange = nsText.lineRange(for: normalizedRange)
    }

    private func scheduleLiveSyntaxHide() {
        liveSyntaxHideTimer?.invalidate()
        guard mode == .live else { return }
        liveSyntaxHideTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: false) { [weak self] _ in
            guard let self, self.mode == .live, !self.isSwitchingDocuments else { return }
            if self.editorTextView.selectedRange().length > 0 {
                return
            }
            self.liveSyntaxRevealRange = nil
            if let index = self.selectedDocumentIndex {
                self.scheduleEditorStyleRefresh(for: self.documents[index])
            }
        }
    }

    private func clearLiveSyntaxReveal() {
        liveSyntaxHideTimer?.invalidate()
        liveSyntaxHideTimer = nil
        liveSyntaxRevealRange = nil
    }

    private func scheduleEditorStyleRefresh(for doc: EditorDocument) {
        pendingEditorStyleWorkItem?.cancel()
        let docID = doc.id
        let workItem = DispatchWorkItem { [weak self] in
            guard let self,
                  self.mode != .preview,
                  let index = self.documents.firstIndex(where: { $0.id == docID }) else {
                return
            }
            self.applyEditorStyleIfNeeded(for: self.documents[index])
        }
        pendingEditorStyleWorkItem = workItem
        DispatchQueue.main.async(execute: workItem)
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
