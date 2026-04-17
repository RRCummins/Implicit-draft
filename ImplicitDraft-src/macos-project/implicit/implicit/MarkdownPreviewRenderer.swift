import Cocoa

enum StandaloneMarkdownPreviewRenderer {
    static func renderPreviewHTML(for doc: EditorDocument, isMarkdown: Bool) -> String {
        let title = MarkdownHTMLRenderer.escapeHTML(doc.title)
        let body: String
        if isMarkdown {
            let blocks = MarkdownParser.parse(doc.text)
            body = MarkdownHTMLRenderer.renderBody(from: blocks)
        } else {
            body = "<pre><code>\(MarkdownHTMLRenderer.escapeHTML(doc.text))</code></pre>"
        }

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
              --bg: \(MarkdownHTMLRenderer.cssHex(AppPalette.windowBg));
              --panel: \(MarkdownHTMLRenderer.cssHex(AppPalette.sidebarBg));
              --text: \(MarkdownHTMLRenderer.cssHex(AppPalette.textPrimary));
              --muted: \(MarkdownHTMLRenderer.cssHex(AppPalette.textMuted));
              --border: \(MarkdownHTMLRenderer.cssHex(AppPalette.border));
              --accent: \(MarkdownHTMLRenderer.cssHex(AppPalette.accent));
              --code: \(MarkdownHTMLRenderer.cssHex(AppPalette.tabActiveBg));
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
            p, ul, ol, pre, blockquote, .callout, .mermaid-block { margin: 0 0 1rem; }
            ul, ol { padding-left: 1.4rem; }
            li > ul, li > ol {
              margin-top: 0.45rem;
              margin-bottom: 0.45rem;
            }
            li > p:first-child {
              margin-top: 0;
            }
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
            .code-block, .mermaid-block {
              border: 1px solid var(--border);
              border-radius: 12px;
              overflow: hidden;
              background: var(--code);
            }
            .code-label {
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
            .code-block pre, .mermaid-block pre {
              margin: 0;
              border: 0;
              border-radius: 0;
            }
            pre code {
              background: transparent;
              border: 0;
              padding: 0;
            }
            .tok-keyword { color: #ff7ab2; }
            .tok-type { color: #78c2ff; }
            .tok-string { color: #a7f3a1; }
            .tok-comment { color: #8b949e; }
            .tok-number { color: #f6c177; }
            .tok-property { color: #8ad4ff; }
            blockquote {
              border-left: 3px solid var(--accent);
              padding: 2px 0 2px 14px;
              color: var(--muted);
              background: color-mix(in srgb, var(--panel) 18%, transparent);
              border-radius: 0 10px 10px 0;
            }
            blockquote > :last-child {
              margin-bottom: 0;
            }
            .callout {
              padding: 14px 16px 14px 18px;
              border: 1px solid var(--border);
              border-left: 3px solid var(--accent);
              border-radius: 14px;
              background: color-mix(in srgb, var(--panel) 42%, transparent);
            }
            .callout-title {
              display: flex;
              align-items: center;
              gap: 10px;
              margin-bottom: 10px;
              font-size: 12px;
              font-weight: 700;
              letter-spacing: 0.05em;
              text-transform: uppercase;
            }
            .callout-icon {
              width: 8px;
              height: 8px;
              border-radius: 999px;
              background: currentColor;
            }
            .callout-content > :last-child {
              margin-bottom: 0;
            }
            .callout-note { color: #78c2ff; }
            .callout-tip { color: #78f0b0; }
            .callout-success { color: #78f0b0; }
            .callout-warning { color: #f6c177; }
            .callout-danger { color: #ff8f8f; }
            .callout-important { color: #c4a1ff; }
            .callout-quote { color: var(--muted); }
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
            a:hover {
              text-decoration: underline;
            }
            .wikilink {
              font-weight: 600;
            }
            img {
              display: block;
              max-width: 100%;
              max-height: 520px;
              height: auto;
              border: 1px solid var(--border);
              border-radius: 14px;
              margin: 0 0 1rem;
              object-fit: contain;
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
            .mermaid-note {
              padding: 10px 12px 12px;
              color: var(--muted);
              font-size: 12px;
              border-top: 1px solid var(--border);
              background: color-mix(in srgb, var(--panel) 48%, transparent);
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
}
