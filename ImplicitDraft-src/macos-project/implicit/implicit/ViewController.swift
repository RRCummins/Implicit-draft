//
//  ViewController.swift
//  implicit
//
//  Created by Ryan Cummins on 3/28/26.
//

import Cocoa

private struct EditorDocument {
    let id: UUID
    var url: URL?
    var title: String
    var text: String
    var isDirty: Bool

    static func untitled() -> EditorDocument {
        EditorDocument(
            id: UUID(),
            url: nil,
            title: "Untitled",
            text: "",
            isDirty: false
        )
    }

    var displayTitle: String {
        isDirty ? "\(title) •" : title
    }
}

private enum EditorMode: Int {
    case source = 0
    case preview = 1
}

final class ViewController: NSViewController, NSTableViewDataSource, NSTableViewDelegate, NSTextViewDelegate, NSMenuItemValidation {
    private enum Metrics {
        static let sidebarWidth: CGFloat = 220
        static let headerHeight: CGFloat = 52
        static let tabStripHeight: CGFloat = 42
        static let statusHeight: CGFloat = 28
        static let inset: CGFloat = 18
        static let cornerRadius: CGFloat = 14
        static let previewTextWidth: CGFloat = 760
    }

    private enum Palette {
        static let window = NSColor(calibratedWhite: 0.11, alpha: 1)
        static let canvas = NSColor(calibratedWhite: 0.08, alpha: 1)
        static let editor = NSColor(calibratedWhite: 0.12, alpha: 1)
        static let border = NSColor(calibratedWhite: 0.22, alpha: 1)
        static let caret = NSColor(calibratedRed: 0.91, green: 0.80, blue: 0.57, alpha: 1)
    }

    private let splitView = NSSplitView()
    private let documentsTableView = NSTableView(frame: .zero)
    private let editorTextView = NSTextView(frame: .zero)
    private let editorScrollView = NSScrollView()
    private let tabStripStack = NSStackView()
    private let titleLabel = NSTextField(labelWithString: "Implicit")
    private let subtitleLabel = NSTextField(labelWithString: "Standalone macOS editor")
    private let pathLabel = NSTextField(labelWithString: "Untitled draft")
    private let statusLabel = NSTextField(labelWithString: "Ready")
    private let sidebarMetaLabel = NSTextField(labelWithString: "0 documents")
    private let newButton = NSButton(title: "New", target: nil, action: nil)
    private let openButton = NSButton(title: "Open", target: nil, action: nil)
    private let saveButton = NSButton(title: "Save", target: nil, action: nil)
    private let emptyStateView = NSVisualEffectView()
    private let emptyTitleLabel = NSTextField(labelWithString: "Start a draft")
    private let emptyBodyLabel = NSTextField(labelWithString: "Write immediately, or open an existing note into this workspace.")
    private let emptyOpenButton = NSButton(title: "Open File", target: nil, action: nil)
    private let modeControl = NSSegmentedControl(labels: ["Source", "Preview"], trackingMode: .selectOne, target: nil, action: nil)

    private var documents: [EditorDocument] = []
    private var selectedDocumentID: UUID?
    private var isSwitchingDocuments = false
    private var mode: EditorMode = .source
    private var didSetInitialSplit = false

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
        window.backgroundColor = Palette.window
        window.appearance = NSAppearance(named: .darkAqua)
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        guard !didSetInitialSplit, splitView.subviews.count > 1 else { return }
        splitView.setPosition(Metrics.sidebarWidth, ofDividerAt: 0)
        didSetInitialSplit = true
    }

    func applicationOpenFiles(_ urls: [URL]) {
        openDocuments(urls)
    }

    func applicationCreateNewDocument() {
        newDocument(nil)
    }

    override var representedObject: Any? {
        didSet {}
    }

    @IBAction func newDocument(_ sender: Any?) {
        let document = EditorDocument.untitled()
        documents.insert(document, at: 0)
        selectedDocumentID = document.id
        documentsTableView.reloadData()
        documentsTableView.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        updateVisibleDocument()
    }

    @IBAction func openDocument(_ sender: Any?) {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        panel.allowsMultipleSelection = true
        panel.allowedContentTypes = []
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
            noteRecent(url)
        } catch {
            statusLabel.stringValue = "Failed to revert: \(error.localizedDescription)"
        }
    }

    @IBAction func changeMode(_ sender: Any?) {
        mode = EditorMode(rawValue: modeControl.selectedSegment) ?? .source
        updateVisibleDocument()
    }

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

    func numberOfRows(in tableView: NSTableView) -> Int {
        documents.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let identifier = NSUserInterfaceItemIdentifier("DocumentCell")
        let textField: NSTextField
        if let cell = tableView.makeView(withIdentifier: identifier, owner: self) as? NSTextField {
            textField = cell
        } else {
            textField = NSTextField(labelWithString: "")
            textField.identifier = identifier
            textField.font = NSFont.systemFont(ofSize: 13, weight: .medium)
            textField.textColor = .labelColor
            textField.lineBreakMode = .byTruncatingMiddle
            textField.translatesAutoresizingMaskIntoConstraints = false

            let container = NSTableCellView()
            container.identifier = identifier
            container.addSubview(textField)
            NSLayoutConstraint.activate([
                textField.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 14),
                textField.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -8),
                textField.centerYAnchor.constraint(equalTo: container.centerYAnchor)
            ])
            return container
        }

        textField.stringValue = documents[row].displayTitle
        return textField.superview
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        let row = documentsTableView.selectedRow
        guard documents.indices.contains(row) else { return }
        selectedDocumentID = documents[row].id
        updateVisibleDocument()
    }

    func textDidChange(_ notification: Notification) {
        guard !isSwitchingDocuments, let index = selectedDocumentIndex else { return }
        documents[index].text = editorTextView.string
        documents[index].isDirty = true
        titleLabel.stringValue = documents[index].displayTitle
        updateWindowTitle()
        documentsTableView.reloadData()
        statusLabel.stringValue = "Edited \(documents[index].title)"
    }

    private var selectedDocumentIndex: Int? {
        guard let selectedDocumentID else { return nil }
        return documents.firstIndex { $0.id == selectedDocumentID }
    }

    private func buildInterface() {
        view.wantsLayer = true
        view.layer?.backgroundColor = Palette.window.cgColor

        let root = NSStackView()
        root.orientation = .vertical
        root.translatesAutoresizingMaskIntoConstraints = false
        root.spacing = 0
        view.addSubview(root)

        let header = makeHeaderView()
        let tabStrip = makeTabStripView()
        let body = makeBodyView()
        let status = makeStatusView()

        root.addArrangedSubview(header)
        root.addArrangedSubview(tabStrip)
        root.addArrangedSubview(body)
        root.addArrangedSubview(status)

        NSLayoutConstraint.activate([
            root.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            root.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            root.topAnchor.constraint(equalTo: view.topAnchor),
            root.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            header.heightAnchor.constraint(equalToConstant: Metrics.headerHeight),
            tabStrip.heightAnchor.constraint(equalToConstant: Metrics.tabStripHeight),
            status.heightAnchor.constraint(equalToConstant: Metrics.statusHeight)
        ])
    }

    private func makeHeaderView() -> NSView {
        let header = NSVisualEffectView()
        header.material = .headerView
        header.blendingMode = .behindWindow
        header.state = .active

        let stack = NSStackView()
        stack.orientation = .horizontal
        stack.alignment = .centerY
        stack.distribution = .fill
        stack.spacing = 12
        stack.translatesAutoresizingMaskIntoConstraints = false
        header.addSubview(stack)

        let titleStack = NSStackView()
        titleStack.orientation = .vertical
        titleStack.spacing = 1
        titleStack.translatesAutoresizingMaskIntoConstraints = false

        titleLabel.font = NSFont.systemFont(ofSize: 18, weight: .semibold)
        titleLabel.textColor = .labelColor
        subtitleLabel.font = NSFont.systemFont(ofSize: 11, weight: .medium)
        subtitleLabel.textColor = .secondaryLabelColor
        titleStack.addArrangedSubview(titleLabel)
        titleStack.addArrangedSubview(subtitleLabel)

        let spacer = NSView()
        spacer.translatesAutoresizingMaskIntoConstraints = false

        modeControl.segmentStyle = .texturedRounded
        modeControl.controlSize = .small
        modeControl.selectedSegment = 0
        modeControl.target = self
        modeControl.action = #selector(changeMode(_:))

        stack.addArrangedSubview(titleStack)
        stack.addArrangedSubview(spacer)
        stack.addArrangedSubview(modeControl)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: header.leadingAnchor, constant: Metrics.inset),
            stack.trailingAnchor.constraint(equalTo: header.trailingAnchor, constant: -Metrics.inset),
            stack.topAnchor.constraint(equalTo: header.topAnchor, constant: 8),
            stack.bottomAnchor.constraint(equalTo: header.bottomAnchor, constant: -8)
        ])

        return header
    }

    private func makeTabStripView() -> NSView {
        let tabStrip = NSVisualEffectView()
        tabStrip.material = .headerView
        tabStrip.blendingMode = .behindWindow
        tabStrip.state = .active

        let scrollView = NSScrollView()
        scrollView.drawsBackground = false
        scrollView.hasHorizontalScroller = true
        scrollView.hasVerticalScroller = false
        scrollView.borderType = .noBorder
        scrollView.translatesAutoresizingMaskIntoConstraints = false
        tabStrip.addSubview(scrollView)

        let content = NSView()
        content.translatesAutoresizingMaskIntoConstraints = false
        scrollView.documentView = content

        tabStripStack.orientation = .horizontal
        tabStripStack.alignment = .centerY
        tabStripStack.spacing = 8
        tabStripStack.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(tabStripStack)

        NSLayoutConstraint.activate([
            scrollView.leadingAnchor.constraint(equalTo: tabStrip.leadingAnchor, constant: Metrics.inset),
            scrollView.trailingAnchor.constraint(equalTo: tabStrip.trailingAnchor, constant: -Metrics.inset),
            scrollView.topAnchor.constraint(equalTo: tabStrip.topAnchor, constant: 6),
            scrollView.bottomAnchor.constraint(equalTo: tabStrip.bottomAnchor, constant: -6),
            content.leadingAnchor.constraint(equalTo: scrollView.contentView.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: scrollView.contentView.trailingAnchor),
            content.topAnchor.constraint(equalTo: scrollView.contentView.topAnchor),
            content.bottomAnchor.constraint(equalTo: scrollView.contentView.bottomAnchor),
            content.heightAnchor.constraint(equalTo: scrollView.contentView.heightAnchor),
            tabStripStack.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            tabStripStack.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            tabStripStack.topAnchor.constraint(equalTo: content.topAnchor),
            tabStripStack.bottomAnchor.constraint(equalTo: content.bottomAnchor)
        ])

        return tabStrip
    }

    private func makeBodyView() -> NSView {
        let container = NSView()
        container.translatesAutoresizingMaskIntoConstraints = false

        splitView.isVertical = true
        splitView.dividerStyle = .thin
        splitView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(splitView)

        let sidebarContainer = NSVisualEffectView()
        sidebarContainer.material = .sidebar
        sidebarContainer.blendingMode = .behindWindow
        sidebarContainer.state = .active

        let sidebarHeader = NSTextField(labelWithString: "Documents")
        sidebarHeader.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
        sidebarHeader.textColor = .secondaryLabelColor
        sidebarHeader.translatesAutoresizingMaskIntoConstraints = false

        let sidebarActionRow = NSStackView()
        sidebarActionRow.orientation = .horizontal
        sidebarActionRow.alignment = .centerY
        sidebarActionRow.spacing = 8
        sidebarActionRow.translatesAutoresizingMaskIntoConstraints = false

        configureHeaderButton(newButton, action: #selector(newDocument(_:)))
        configureHeaderButton(openButton, action: #selector(openDocument(_:)))
        configureHeaderButton(saveButton, action: #selector(saveDocument(_:)))

        sidebarActionRow.addArrangedSubview(newButton)
        sidebarActionRow.addArrangedSubview(openButton)
        sidebarActionRow.addArrangedSubview(saveButton)

        let sidebarHeaderRow = NSStackView()
        sidebarHeaderRow.orientation = .horizontal
        sidebarHeaderRow.alignment = .centerY
        sidebarHeaderRow.distribution = .fill
        sidebarHeaderRow.spacing = 8
        sidebarHeaderRow.translatesAutoresizingMaskIntoConstraints = false

        let sidebarSpacer = NSView()
        sidebarSpacer.translatesAutoresizingMaskIntoConstraints = false
        sidebarMetaLabel.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        sidebarMetaLabel.textColor = .tertiaryLabelColor

        sidebarHeaderRow.addArrangedSubview(sidebarHeader)
        sidebarHeaderRow.addArrangedSubview(sidebarSpacer)
        sidebarHeaderRow.addArrangedSubview(sidebarMetaLabel)

        let sidebarSection = NSView()
        sidebarSection.wantsLayer = true
        sidebarSection.layer?.cornerRadius = Metrics.cornerRadius - 4
        sidebarSection.layer?.borderWidth = 1
        sidebarSection.layer?.borderColor = Palette.border.cgColor
        sidebarSection.layer?.backgroundColor = NSColor(calibratedWhite: 0.13, alpha: 0.9).cgColor
        sidebarSection.translatesAutoresizingMaskIntoConstraints = false
        sidebarContainer.addSubview(sidebarSection)

        documentsTableView.headerView = nil
        documentsTableView.style = .sourceList
        documentsTableView.selectionHighlightStyle = .regular
        documentsTableView.rowHeight = 32
        documentsTableView.focusRingType = .none
        documentsTableView.backgroundColor = .clear
        documentsTableView.intercellSpacing = NSSize(width: 0, height: 4)
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("documents"))
        documentsTableView.addTableColumn(column)
        documentsTableView.delegate = self
        documentsTableView.dataSource = self

        let sidebarScroll = NSScrollView()
        sidebarScroll.hasVerticalScroller = true
        sidebarScroll.drawsBackground = false
        sidebarScroll.translatesAutoresizingMaskIntoConstraints = false
        sidebarScroll.documentView = documentsTableView
        sidebarSection.addSubview(sidebarScroll)

        let sidebarStack = NSStackView(views: [sidebarActionRow, sidebarHeaderRow, sidebarSection])
        sidebarStack.orientation = .vertical
        sidebarStack.spacing = 14
        sidebarStack.translatesAutoresizingMaskIntoConstraints = false
        sidebarContainer.addSubview(sidebarStack)

        NSLayoutConstraint.activate([
            sidebarStack.leadingAnchor.constraint(equalTo: sidebarContainer.leadingAnchor, constant: 14),
            sidebarStack.trailingAnchor.constraint(equalTo: sidebarContainer.trailingAnchor, constant: -14),
            sidebarStack.topAnchor.constraint(equalTo: sidebarContainer.topAnchor, constant: 14),
            sidebarStack.bottomAnchor.constraint(equalTo: sidebarContainer.bottomAnchor, constant: -14),
            sidebarScroll.leadingAnchor.constraint(equalTo: sidebarSection.leadingAnchor, constant: 4),
            sidebarScroll.trailingAnchor.constraint(equalTo: sidebarSection.trailingAnchor, constant: -4),
            sidebarScroll.topAnchor.constraint(equalTo: sidebarSection.topAnchor, constant: 6),
            sidebarScroll.bottomAnchor.constraint(equalTo: sidebarSection.bottomAnchor, constant: -6)
        ])

        let editorContainer = NSView()
        editorContainer.wantsLayer = true
        editorContainer.layer?.backgroundColor = Palette.canvas.cgColor

        let editorCard = NSVisualEffectView()
        editorCard.material = .windowBackground
        editorCard.blendingMode = .withinWindow
        editorCard.state = .active
        editorCard.wantsLayer = true
        editorCard.layer?.cornerRadius = Metrics.cornerRadius
        editorCard.layer?.borderWidth = 1
        editorCard.layer?.borderColor = Palette.border.cgColor
        editorCard.layer?.backgroundColor = Palette.editor.cgColor
        editorCard.translatesAutoresizingMaskIntoConstraints = false
        editorContainer.addSubview(editorCard)

        pathLabel.font = NSFont.systemFont(ofSize: 12, weight: .medium)
        pathLabel.textColor = .secondaryLabelColor
        pathLabel.translatesAutoresizingMaskIntoConstraints = false
        editorCard.addSubview(pathLabel)

        editorScrollView.hasVerticalScroller = true
        editorScrollView.hasHorizontalScroller = true
        editorScrollView.borderType = .noBorder
        editorScrollView.drawsBackground = false
        editorScrollView.translatesAutoresizingMaskIntoConstraints = false
        editorCard.addSubview(editorScrollView)

        editorTextView.isRichText = false
        editorTextView.isAutomaticQuoteSubstitutionEnabled = false
        editorTextView.isAutomaticDataDetectionEnabled = false
        editorTextView.isContinuousSpellCheckingEnabled = false
        editorTextView.usesFindBar = true
        editorTextView.font = NSFont.monospacedSystemFont(ofSize: 14, weight: .regular)
        editorTextView.textColor = .labelColor
        editorTextView.backgroundColor = Palette.editor
        editorTextView.insertionPointColor = Palette.caret
        editorTextView.allowsUndo = true
        editorTextView.textContainerInset = NSSize(width: 14, height: 14)
        editorTextView.isHorizontallyResizable = true
        editorTextView.isVerticallyResizable = true
        editorTextView.delegate = self
        editorScrollView.documentView = editorTextView

        emptyStateView.material = .menu
        emptyStateView.blendingMode = .withinWindow
        emptyStateView.state = .active
        emptyStateView.wantsLayer = true
        emptyStateView.layer?.cornerRadius = Metrics.cornerRadius
        emptyStateView.layer?.borderWidth = 1
        emptyStateView.layer?.borderColor = Palette.border.cgColor
        emptyStateView.translatesAutoresizingMaskIntoConstraints = false
        editorCard.addSubview(emptyStateView)

        let emptyStack = NSStackView()
        emptyStack.orientation = .vertical
        emptyStack.alignment = .leading
        emptyStack.spacing = 10
        emptyStack.translatesAutoresizingMaskIntoConstraints = false
        emptyStateView.addSubview(emptyStack)

        emptyTitleLabel.font = NSFont.systemFont(ofSize: 22, weight: .semibold)
        emptyTitleLabel.textColor = .labelColor

        emptyBodyLabel.font = NSFont.systemFont(ofSize: 13, weight: .regular)
        emptyBodyLabel.textColor = .secondaryLabelColor
        emptyBodyLabel.maximumNumberOfLines = 0
        emptyBodyLabel.lineBreakMode = .byWordWrapping

        configureHeaderButton(emptyOpenButton, action: #selector(openDocument(_:)))

        emptyStack.addArrangedSubview(emptyTitleLabel)
        emptyStack.addArrangedSubview(emptyBodyLabel)
        emptyStack.addArrangedSubview(emptyOpenButton)

        NSLayoutConstraint.activate([
            editorCard.leadingAnchor.constraint(equalTo: editorContainer.leadingAnchor, constant: Metrics.inset),
            editorCard.trailingAnchor.constraint(equalTo: editorContainer.trailingAnchor, constant: -Metrics.inset),
            editorCard.topAnchor.constraint(equalTo: editorContainer.topAnchor, constant: Metrics.inset),
            editorCard.bottomAnchor.constraint(equalTo: editorContainer.bottomAnchor, constant: -Metrics.inset),
            pathLabel.leadingAnchor.constraint(equalTo: editorCard.leadingAnchor, constant: 18),
            pathLabel.trailingAnchor.constraint(equalTo: editorCard.trailingAnchor, constant: -18),
            pathLabel.topAnchor.constraint(equalTo: editorCard.topAnchor, constant: 14),
            editorScrollView.leadingAnchor.constraint(equalTo: editorCard.leadingAnchor, constant: 10),
            editorScrollView.trailingAnchor.constraint(equalTo: editorCard.trailingAnchor, constant: -10),
            editorScrollView.topAnchor.constraint(equalTo: pathLabel.bottomAnchor, constant: 10),
            editorScrollView.bottomAnchor.constraint(equalTo: editorCard.bottomAnchor, constant: -10),
            emptyStateView.centerXAnchor.constraint(equalTo: editorCard.centerXAnchor),
            emptyStateView.centerYAnchor.constraint(equalTo: editorCard.centerYAnchor),
            emptyStateView.widthAnchor.constraint(equalToConstant: 420),
            emptyStack.leadingAnchor.constraint(equalTo: emptyStateView.leadingAnchor, constant: 22),
            emptyStack.trailingAnchor.constraint(equalTo: emptyStateView.trailingAnchor, constant: -22),
            emptyStack.topAnchor.constraint(equalTo: emptyStateView.topAnchor, constant: 22),
            emptyStack.bottomAnchor.constraint(equalTo: emptyStateView.bottomAnchor, constant: -22)
        ])

        splitView.addArrangedSubview(sidebarContainer)
        splitView.addArrangedSubview(editorContainer)
        splitView.setHoldingPriority(.defaultLow, forSubviewAt: 0)

        NSLayoutConstraint.activate([
            splitView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            splitView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            splitView.topAnchor.constraint(equalTo: container.topAnchor),
            splitView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            sidebarContainer.widthAnchor.constraint(equalToConstant: Metrics.sidebarWidth)
        ])

        return container
    }

    private func makeStatusView() -> NSView {
        let status = NSVisualEffectView()
        status.material = .headerView
        status.blendingMode = .behindWindow
        status.state = .active

        statusLabel.font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        statusLabel.textColor = .secondaryLabelColor
        statusLabel.translatesAutoresizingMaskIntoConstraints = false
        status.addSubview(statusLabel)

        NSLayoutConstraint.activate([
            statusLabel.leadingAnchor.constraint(equalTo: status.leadingAnchor, constant: Metrics.inset),
            statusLabel.trailingAnchor.constraint(equalTo: status.trailingAnchor, constant: -Metrics.inset),
            statusLabel.centerYAnchor.constraint(equalTo: status.centerYAnchor)
        ])

        return status
    }

    private func configureHeaderButton(_ button: NSButton, action: Selector) {
        button.bezelStyle = .texturedRounded
        button.controlSize = .small
        button.target = self
        button.action = action
    }

    private func seedInitialDocumentIfNeeded() {
        guard documents.isEmpty else { return }
        let initial = EditorDocument.untitled()
        documents = [initial]
        selectedDocumentID = initial.id
        documentsTableView.reloadData()
        documentsTableView.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
        refreshTabStrip()
    }

    private func updateVisibleDocument() {
        guard let index = selectedDocumentIndex else {
            editorTextView.string = ""
            pathLabel.stringValue = "No document selected"
            statusLabel.stringValue = "Ready"
            sidebarMetaLabel.stringValue = "0 documents"
            refreshTabStrip()
            return
        }

        isSwitchingDocuments = true
        let document = documents[index]
        applyDocumentContent(document)
        pathLabel.stringValue = document.url?.path(percentEncoded: false) ?? "Untitled draft"
        titleLabel.stringValue = document.displayTitle
        subtitleLabel.stringValue = document.url?.deletingLastPathComponent().path(percentEncoded: false) ?? "Standalone document"
        statusLabel.stringValue = "Viewing \(document.title) in \(mode == .source ? "source" : "preview")"
        sidebarMetaLabel.stringValue = "\(documents.count) document\(documents.count == 1 ? "" : "s")"
        updateWindowTitle()
        refreshTabStrip()
        isSwitchingDocuments = false
    }

    private func updateWindowTitle() {
        view.window?.title = documents[selectedDocumentIndex ?? 0].displayTitle
    }

    private func openDocuments(_ urls: [URL]) {
        guard !urls.isEmpty else { return }
        for url in urls {
            do {
                let text = try String(contentsOf: url, encoding: .utf8)
                if let existingIndex = documents.firstIndex(where: { $0.url == url }) {
                    documents[existingIndex].text = text
                    documents[existingIndex].isDirty = false
                    selectedDocumentID = documents[existingIndex].id
                } else {
                    let document = EditorDocument(
                        id: UUID(),
                        url: url,
                        title: url.lastPathComponent,
                        text: text,
                        isDirty: false
                    )
                    documents.append(document)
                    selectedDocumentID = document.id
                }
                noteRecent(url)
            } catch {
                statusLabel.stringValue = "Failed to open \(url.lastPathComponent): \(error.localizedDescription)"
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
            noteRecent(url)
            documentsTableView.reloadData()
            updateVisibleDocument()
        } catch {
            statusLabel.stringValue = "Failed to save \(url.lastPathComponent): \(error.localizedDescription)"
        }
    }

    private func noteRecent(_ url: URL) {
        NSDocumentController.shared.noteNewRecentDocumentURL(url)
    }

    private func suggestedFilename(for document: EditorDocument) -> String {
        if let url = document.url {
            return url.lastPathComponent
        }
        return document.title == "Untitled" ? "Untitled.md" : document.title
    }

    private func applyDocumentContent(_ document: EditorDocument) {
        let shouldShowEmptyState = document.url == nil && document.text.isEmpty && mode == .source
        emptyStateView.isHidden = !shouldShowEmptyState
        editorScrollView.isHidden = shouldShowEmptyState

        switch mode {
        case .source:
            editorTextView.isEditable = true
            editorTextView.isSelectable = true
            editorScrollView.hasHorizontalScroller = true
            editorTextView.string = document.text
            editorTextView.font = NSFont.monospacedSystemFont(ofSize: 14, weight: .regular)
            editorTextView.textColor = .labelColor
            editorTextView.alignment = .left
            editorTextView.textContainerInset = NSSize(width: 14, height: 14)
            editorTextView.isHorizontallyResizable = true
            editorTextView.textContainer?.widthTracksTextView = false
            editorTextView.textContainer?.containerSize = NSSize(
                width: CGFloat.greatestFiniteMagnitude,
                height: CGFloat.greatestFiniteMagnitude
            )
        case .preview:
            editorTextView.isEditable = false
            editorTextView.isSelectable = true
            editorScrollView.hasHorizontalScroller = false
            editorTextView.alignment = .left
            editorTextView.textContainerInset = NSSize(width: 36, height: 28)
            editorTextView.isHorizontallyResizable = false
            editorTextView.textContainer?.widthTracksTextView = true
            editorTextView.textContainer?.containerSize = NSSize(
                width: Metrics.previewTextWidth,
                height: CGFloat.greatestFiniteMagnitude
            )
            editorTextView.textStorage?.setAttributedString(renderPreview(for: document))
        }
    }

    @objc
    private func selectTabFromStrip(_ sender: NSButton) {
        let index = sender.tag
        guard documents.indices.contains(index) else { return }
        selectedDocumentID = documents[index].id
        documentsTableView.selectRowIndexes(IndexSet(integer: index), byExtendingSelection: false)
        updateVisibleDocument()
    }

    @objc
    private func createTabFromStrip(_ sender: Any?) {
        newDocument(sender)
    }

    private func refreshTabStrip() {
        tabStripStack.arrangedSubviews.forEach { view in
            tabStripStack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        for (index, document) in documents.enumerated() {
            let button = NSButton(title: document.displayTitle, target: self, action: #selector(selectTabFromStrip(_:)))
            button.tag = index
            button.isBordered = false
            button.font = NSFont.systemFont(ofSize: 12, weight: .medium)
            button.contentTintColor = index == selectedDocumentIndex ? .labelColor : .secondaryLabelColor
            button.wantsLayer = true
            button.layer?.cornerRadius = 8
            button.layer?.backgroundColor = (index == selectedDocumentIndex
                ? NSColor(calibratedWhite: 0.18, alpha: 1)
                : NSColor(calibratedWhite: 0.14, alpha: 0.65)).cgColor
            button.imagePosition = .imageLeading
            button.setButtonType(.momentaryPushIn)
            button.bezelStyle = .regularSquare
            button.translatesAutoresizingMaskIntoConstraints = false
            NSLayoutConstraint.activate([
                button.heightAnchor.constraint(equalToConstant: 28),
                button.widthAnchor.constraint(greaterThanOrEqualToConstant: 110)
            ])
            tabStripStack.addArrangedSubview(button)
        }

        let addButton = NSButton(title: "+", target: self, action: #selector(createTabFromStrip(_:)))
        addButton.isBordered = false
        addButton.font = NSFont.systemFont(ofSize: 14, weight: .semibold)
        addButton.contentTintColor = .secondaryLabelColor
        addButton.wantsLayer = true
        addButton.layer?.cornerRadius = 8
        addButton.layer?.backgroundColor = NSColor(calibratedWhite: 0.13, alpha: 0.85).cgColor
        addButton.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            addButton.widthAnchor.constraint(equalToConstant: 28),
            addButton.heightAnchor.constraint(equalToConstant: 28)
        ])
        tabStripStack.addArrangedSubview(addButton)
    }

    private func renderPreview(for document: EditorDocument) -> NSAttributedString {
        if isMarkdownDocument(document),
           let attributed = try? AttributedString(markdown: document.text) {
            let rendered = NSMutableAttributedString(attributedString: NSAttributedString(attributed))
            let fullRange = NSRange(location: 0, length: rendered.length)
            let paragraph = NSMutableParagraphStyle()
            paragraph.lineHeightMultiple = 1.15
            paragraph.paragraphSpacing = 10
            paragraph.paragraphSpacingBefore = 2
            rendered.addAttribute(.paragraphStyle, value: paragraph, range: fullRange)
            rendered.addAttribute(.foregroundColor, value: NSColor.labelColor, range: fullRange)
            return rendered
        }

        let paragraph = NSMutableParagraphStyle()
        paragraph.lineHeightMultiple = 1.18
        paragraph.paragraphSpacing = 8

        return NSAttributedString(
            string: document.text,
            attributes: [
                .font: NSFont.systemFont(ofSize: 15, weight: .regular),
                .foregroundColor: NSColor.labelColor,
                .paragraphStyle: paragraph
            ]
        )
    }

    private func isMarkdownDocument(_ document: EditorDocument) -> Bool {
        guard let pathExtension = document.url?.pathExtension.lowercased() else {
            return true
        }
        return ["md", "markdown", "mdown", "txt"].contains(pathExtension)
    }
}
