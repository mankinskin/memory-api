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

# Snapshot existing Code.exe PIDs before installation so we can kill only
# the window that `code --install-extension` opens, not all VS Code instances.
get_code_pids() {
  tasklist.exe /FI "IMAGENAME eq Code.exe" /FO CSV /NH 2>/dev/null \
    | grep -i "code.exe" \
    | awk -F',' '{print $2}' \
    | tr -d '"' \
    | sort -n
}

BEFORE_PIDS="$(get_code_pids)"
code --install-extension "$VSIX" --force
# Give Code.exe a moment to finish spawning its window process.
sleep 2
AFTER_PIDS="$(get_code_pids)"

# Determine PIDs that appeared after the install command ran.
NEW_PIDS="$(comm -13 <(echo "$BEFORE_PIDS") <(echo "$AFTER_PIDS") || true)"

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
  echo "==> Syncing out/, resources/ and package.json to ${INSTALL_DIR_UNIX} ..."
  cp -r out/. "${INSTALL_DIR_UNIX}/out/"
  cp -r resources/. "${INSTALL_DIR_UNIX}/resources/"
  cp package.json "${INSTALL_DIR_UNIX}/package.json"
  echo "==> Sync complete."
fi

if [[ -n "$NEW_PIDS" ]]; then
  echo "==> Closing VS Code window opened by installer..."
  for pid in $NEW_PIDS; do
    taskkill.exe /F /PID "$pid" 2>/dev/null || true
  done
else
  echo "==> No new VS Code window detected; nothing to close."
fi

echo "==> Done. Reload VS Code (or reopen it) to activate the updated extension."
