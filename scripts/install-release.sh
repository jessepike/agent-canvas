#!/usr/bin/env bash
# Build the AgentCanvas release bundle and install it to /Applications.
#
# This produces a real macOS .app (unsigned, ad-hoc) and registers it with
# LaunchServices so Spotlight, Launchpad, and `open -a AgentCanvas` see it.
# Companion to ./launch-dev.sh, which runs the debug binary + vite.
#
# Usage:
#   ./scripts/install-release.sh            # build, install, register, launch
#   ./scripts/install-release.sh --no-open  # build + install but don't launch
#
# Requires:
#   - cargo + rustc at ~/.cargo/bin (added to PATH automatically)
#   - @tauri-apps/cli installed in ui/node_modules (via `pnpm add -D @tauri-apps/cli@^2`)
#   - Existing dev binary + vite are killed before launch (they hold the MCP socket)
#
# First build is slow (5–15 min, cold cargo release compile). Subsequent
# builds are 10–60 s.

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

NO_OPEN=false
for arg in "$@"; do
  case "$arg" in
    --no-open) NO_OPEN=true ;;
    -h|--help)
      grep -E '^# ' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "unknown flag: $arg" >&2
      exit 2
      ;;
  esac
done

# 1. PATH for cargo
export PATH="$HOME/.cargo/bin:$PATH"

# 2. Sanity checks
if [ ! -x "$HOME/.cargo/bin/cargo" ]; then
  echo "✗ cargo not found at ~/.cargo/bin/cargo — install rustup first" >&2
  exit 1
fi
if [ ! -x "$REPO/ui/node_modules/.bin/tauri" ]; then
  echo "✗ tauri-cli not installed — run: cd ui && pnpm add -D @tauri-apps/cli@^2" >&2
  exit 1
fi

# 3. Stop dev processes — they conflict on the MCP socket and SQLite
pkill -f "target/debug/agent-canvas-app" 2>/dev/null && echo "→ killed dev binary" || true
pkill -f "ui/node_modules/.bin/vite"     2>/dev/null && echo "→ stopped vite"      || true
sleep 0.5

# 4. Build the .app bundle
echo "→ building release bundle (this can take a while on a cold build)"
BUILD_LOG="/tmp/agent-canvas-release-build.log"
if ! "$REPO/ui/node_modules/.bin/tauri" build --bundles app > "$BUILD_LOG" 2>&1; then
  echo "✗ build failed — see $BUILD_LOG" >&2
  tail -30 "$BUILD_LOG" >&2
  exit 1
fi

APP_SRC="$REPO/target/release/bundle/macos/AgentCanvas.app"
if [ ! -d "$APP_SRC" ]; then
  echo "✗ bundle not found at $APP_SRC" >&2
  exit 1
fi

# 5. Install to /Applications (replace prior)
APP_DST="/Applications/AgentCanvas.app"
if [ -d "$APP_DST" ]; then
  rm -rf "$APP_DST"
fi
cp -R "$APP_SRC" "$APP_DST"
echo "→ installed to $APP_DST"

# 6. Strip quarantine xattr so Gatekeeper doesn't block first launch
xattr -dr com.apple.quarantine "$APP_DST" 2>/dev/null || true

# 7. Register with LaunchServices so Spotlight + Launchpad + open -a see it
/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister \
  -f "$APP_DST"
echo "→ registered with LaunchServices"

# 8. Launch
if [ "$NO_OPEN" = "false" ]; then
  open "$APP_DST"
  sleep 1.5
  if pgrep -fl "AgentCanvas.app/Contents/MacOS" >/dev/null; then
    echo "✓ AgentCanvas running"
  else
    echo "✗ AgentCanvas failed to launch — check Console.app" >&2
    exit 1
  fi
else
  echo "✓ install complete; skipping launch (--no-open)"
fi
