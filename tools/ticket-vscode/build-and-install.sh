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

# Derive the installed extension directory name from package.json.
PUBLISHER="$(node -p "require('./package.json').publisher || 'undefined_publisher'")"
NAME="$(node -p "require('./package.json').name")"
VERSION="$(node -p "require('./package.json').version")"
INSTALL_DIR="${USERPROFILE}/.vscode/extensions/${PUBLISHER}.${NAME}-${VERSION}"
INSTALL_DIR_UNIX="$(cygpath -u "${INSTALL_DIR}" 2>/dev/null || echo "${INSTALL_DIR}")"

# Directly sync the compiled output so VS Code always gets the latest build,
# even when the version hasn't changed (code --install-extension skips
# extraction when the folder already exists with the same version).
if [[ -d "$INSTALL_DIR_UNIX" ]]; then
  echo "==> Syncing out/ and package.json to ${INSTALL_DIR_UNIX} ..."
  cp -r out/. "${INSTALL_DIR_UNIX}/out/"
  cp package.json "${INSTALL_DIR_UNIX}/package.json"
  echo "==> Sync complete."
fi

echo "==> Done. Reload VS Code window to activate the updated extension."
