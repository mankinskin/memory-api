#!/usr/bin/env bash
# Build, package, and install the ticket-vscode extension into the running VS Code.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "==> Compiling TypeScript..."
npm run compile

echo "==> Packaging extension..."
vsce package --no-dependencies --allow-missing-repository --skip-license

VSIX="$(ls -t ticket-viewer-*.vsix | head -n1)"
echo "==> Installing $VSIX..."
code --install-extension "$VSIX" --force

echo "==> Done. Reload VS Code window to activate the updated extension."
