#!/usr/bin/env bash
# Launch AgentCanvas in dev mode.
#
# Starts the vite dev server (if not already running) and launches the
# debug binary as a direct subprocess (no LaunchServices, no Gatekeeper
# prompts).
#
# Usage:
#   ./scripts/launch-dev.sh

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

# 1. Start vite dev server if it isn't already on :1420
if ! curl -s -o /dev/null -w "%{http_code}" http://localhost:1420 | grep -q 200; then
  echo "→ starting vite dev server"
  cd "$REPO/ui"
  nohup ./node_modules/.bin/vite > /tmp/vite-dev.log 2>&1 &
  echo $! > /tmp/vite-dev.pid
  cd "$REPO"
  # wait up to 5s for it to come up
  for i in {1..10}; do
    if curl -s -o /dev/null -w "%{http_code}" http://localhost:1420 | grep -q 200; then
      break
    fi
    sleep 0.5
  done
else
  echo "→ vite already running on :1420"
fi

# 2. Kill any existing AgentCanvas instance
pkill -f "agent-canvas-app" 2>/dev/null || true
sleep 0.5

# 3. Launch the debug binary directly (no `open`, no LaunchServices)
echo "→ launching agent-canvas-app"
nohup "$REPO/target/debug/agent-canvas-app" > /tmp/agent-canvas.log 2>&1 &
echo $! > /tmp/agent-canvas.pid
sleep 1
if ps -p "$(cat /tmp/agent-canvas.pid)" > /dev/null; then
  echo "✓ AgentCanvas running, PID $(cat /tmp/agent-canvas.pid)"
else
  echo "✗ AgentCanvas failed to launch — see /tmp/agent-canvas.log"
  exit 1
fi
