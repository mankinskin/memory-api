# spec-vscode Extension

## Objective

VS Code extension for browsing specification files with rich HTML rendering, navigation links, and code reference jump-to-source.

## Features

1. **Tree view**: Specs organized by component and hierarchy in sidebar
2. **Webview panel**: Rich markdown rendering of spec body and sections
3. **Code ref links**: Click to jump to referenced source file/symbol
4. **State badges**: Visual indicators for spec lifecycle state
5. **Search**: Quick-filter specs by title, slug, component
6. **Feature status**: Color-coded feature completeness indicators

## Dependencies

- spec-http for data fetching
- Similar architecture to ticket-vscode

## Acceptance Criteria

- [ ] Extension installs and activates
- [ ] Tree view shows spec hierarchy
- [ ] Webview renders spec body with markdown
- [ ] Code ref links open source files at correct line
- [ ] State badges show current lifecycle state