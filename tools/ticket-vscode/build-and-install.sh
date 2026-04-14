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
# Allow code --install-extension to return non-zero (e.g. devtools port errors)
# without aborting the script; the sync step below is what guarantees files
# are up-to-date regardless of whether extraction actually happened.
code --install-extension "$VSIX" --force || true
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

# Always sync compiled output to the installed directory.
# `code --install-extension --force` skips extraction when the extension folder
# already exists at the same version, so a direct copy is the only reliable way
# to guarantee the running extension matches what was just compiled.
if [[ -d "$INSTALL_DIR_UNIX" ]]; then
  echo "==> Syncing out/, resources/ and package.json to ${INSTALL_DIR_UNIX} ..."
  cp -r out/. "${INSTALL_DIR_UNIX}/out/"
  cp -r resources/. "${INSTALL_DIR_UNIX}/resources/"
  cp package.json "${INSTALL_DIR_UNIX}/package.json"
  echo "==> Sync complete."
else
  echo "==> WARNING: install directory not found at ${INSTALL_DIR_UNIX}" >&2
  echo "==>          Extension may not have been installed correctly." >&2
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
