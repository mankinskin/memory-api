#!/usr/bin/env bash
# Build, package, and install the ticket-vscode extension into the running VS Code.
#
# Strategy: compile, then directly overwrite the installed extension directory
# with the fresh build artifacts. This bypasses `code --install-extension`
# quirks (skipped extraction, stale cache, extra windows) entirely for the
# common dev-loop case where the extension is already installed.
#
# If the install directory doesn't exist yet (first install), falls back to
# `code --install-extension`.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ── Derive install directory ─────────────────────────────────────────────────
PUBLISHER="$(node -p "require('./package.json').publisher || 'undefined_publisher'")"
NAME="$(node -p "require('./package.json').name")"
VERSION="$(node -p "require('./package.json').version")"
INSTALL_DIR="${USERPROFILE}/.vscode/extensions/${PUBLISHER}.${NAME}-${VERSION}"
INSTALL_DIR_UNIX="$(cygpath -u "${INSTALL_DIR}" 2>/dev/null || echo "${INSTALL_DIR}")"

echo "==> Compiling TypeScript..."
npm run compile

# ── Fast path: extension already installed — overwrite in-place ──────────────
if [[ -d "$INSTALL_DIR_UNIX" ]]; then
  echo "==> Extension dir exists at ${INSTALL_DIR_UNIX}"
  echo "==> Cleaning old out/ in install dir..."
  rm -rf "${INSTALL_DIR_UNIX}/out"

  echo "==> Syncing out/, resources/ and package.json..."
  cp -r out "${INSTALL_DIR_UNIX}/out"
  cp -r resources/. "${INSTALL_DIR_UNIX}/resources/"
  cp package.json "${INSTALL_DIR_UNIX}/package.json"
  # Copy node_modules if they exist (e.g. playwright)
  if [[ -d node_modules ]]; then
    cp -r node_modules/. "${INSTALL_DIR_UNIX}/node_modules/"
  fi

  echo "==> Sync complete. Reload VS Code window to activate."
  exit 0
fi

# ── Slow path: first install via VSIX ────────────────────────────────────────
echo "==> Install dir not found — performing first-time VSIX install..."
echo "==> Packaging extension..."
vsce package --no-dependencies --allow-missing-repository --skip-license

VSIX="$(ls -t ticket-viewer-*.vsix | head -n1)"
echo "==> Installing $VSIX..."

# Snapshot existing Code.exe PIDs before installation so we can kill only
# the window that `code --install-extension` opens, not all VS Code instances.
get_code_pids() {
  # Disable pipefail locally so grep exiting 1 (no match) doesn't abort.
  set +o pipefail
  tasklist.exe /FI "IMAGENAME eq Code.exe" /FO CSV /NH 2>/dev/null \
    | grep -i "code.exe" \
    | awk -F',' '{print $2}' \
    | tr -d '"' \
    | sort -n
  set -o pipefail
  return 0
}

BEFORE_PIDS="$(get_code_pids)"
code --install-extension "$VSIX" --force || true
sleep 2
AFTER_PIDS="$(get_code_pids)"

NEW_PIDS="$(comm -13 <(echo "$BEFORE_PIDS") <(echo "$AFTER_PIDS") || true)"

if [[ -n "$NEW_PIDS" ]]; then
  echo "==> Closing VS Code window opened by installer..."
  for pid in $NEW_PIDS; do
    taskkill.exe /F /PID "$pid" 2>/dev/null || true
  done
fi

echo "==> Done. Reload VS Code window to activate the extension."
