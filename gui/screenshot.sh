#!/bin/sh
# macOS only: opens the demo session (no model, no project files) and saves docs/screenshots/gui.png.
# The window is captured by its own rectangle, so nothing else on the desktop ends up in the picture.
set -e
cd "$(dirname "$0")"
cargo build --release
PORTCODE_GUI_DEMO=1 ./target/release/portcode-gui >/dev/null 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null' EXIT
sleep 6
osascript -e 'tell application "System Events" to tell (first process whose unix id is '$PID') to set frontmost to true' >/dev/null
sleep 1
GEOM=$(osascript -e 'tell application "System Events" to tell (first process whose unix id is '$PID') to get {position, size} of window 1')
X=$(echo "$GEOM" | cut -d, -f1 | tr -d ' '); Y=$(echo "$GEOM" | cut -d, -f2 | tr -d ' ')
W=$(echo "$GEOM" | cut -d, -f3 | tr -d ' '); H=$(echo "$GEOM" | cut -d, -f4 | tr -d ' ')
screencapture -x -R"$X,$Y,$W,$H" ../docs/screenshots/gui.png
echo "wrote docs/screenshots/gui.png"
