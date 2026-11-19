#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/unix/SpaceMouseSpnav.cpp app/platform/unix/SpaceMouseMagellan.cpp
# check-all sources: app/platform/unix/SpaceMouseMagellan.hpp app/platform/SpaceMouseDevice.hpp
# check-all sources: app/view/SpaceMouseController.cpp app/MainWindow.cpp app/WindowManager.cpp
# check-all sources: app/DocumentFrame.cpp app/framework/FloatingLayout.cpp
# Linux without a 3D-mouse daemon (mitcad#2059): with no spacenavd and
# SPNAV_SOCKET not set, the 3D mouse's X11 fallback (the Magellan driver
# of 3DxWare) finds no driver, the log says "3D mouse: none", and the
# Floating layout looks as it does without the fallback: just outside the
# Browser's card, the navigation bar and the message band the screen shows
# the view's background, not the black boxes that a design window made
# native too early painted round them. The screen is read from the X
# server (xwd), as a person sees it, not from Qt's own picture of the
# window. Linux only: the fallback is X11 code; Windows and macOS have no
# such path.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-space-mouse-no-daemon-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
STATS=$here/ui-image-stats.py
WORK=$(mktemp -d /tmp/mitcad-ui-space-mouse-no-daemon.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
# The fallback runs only when SPNAV_SOCKET is not set.
unset SPNAV_SOCKET
ui_start_display
UI_DEFAULT_CHROME=1 ui_start_app --no-mcp
# The backend starts once the saved navigation is applied, and the
# fallback looks again every 5 s: a dump after one such look.
sleep 6
ui_sync
if [ -S /var/run/spnav.sock ]; then
  echo "note: a spacenavd socket is on this machine: the 3D mouse may be found"
  ui_expect_log '3D mouse: ' "the 3D mouse's backend started"
else
  ui_expect_log '3D mouse: none' "without a daemon or driver the log says no 3D mouse"
fi
expect_absent() { grep -qF -- "$1" "$UI_LOG" && ui_fail "$2: '$1' in the log"; echo "ok   $2"; }
expect_absent '3DxWare/Magellan X11 connected' "no Magellan driver was found on the bare display"

xdotool mousemove $((X + WIDTH / 2)) $((Y + HEIGHT - 4))
ui_sync
xwd -root -silent > "$WORK/screen.xwd" || ui_fail "no screenshot"
cards=$(grep '^Floating cards: ' "$UI_LOG" | tail -1)
[ -n "$cards" ] || ui_fail "no Floating cards line in the log"

# mean x y: the 4 x 4 pixels' mean colour at x, y of the screen, "r g b".
mean() {
  python3 -c '
import importlib.util, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1])
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
rows = stats.region_rows(sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), 4, 4)
pixels = [p for row in rows for p in row]
print(" ".join("%d" % (sum(p[i] for p in pixels) / len(pixels)) for i in range(3)))' \
    "$STATS" "$WORK/screen.xwd" "$1" "$2"
}

# beside card: 6 px right of the card, at its middle, the screen shows what
# it shows 60 px further right (the view's background): no dark frame.
checked=0
beside() {
  local entry x y w h near far
  entry=$(tr ';' '\n' <<< "$cards" | sed 's/^ *//' | grep "^$1 " | tail -1)
  if [ -z "$entry" ]; then
    echo "note: $1 is not shown"
    return
  fi
  read -r x y w h < <(sed -E 's/^[^ ]+ ([0-9-]+),([0-9-]+) ([0-9]+)x([0-9]+).*/\1 \2 \3 \4/' <<< "$entry")
  near=$(mean $((X + x + w + 6)) $((Y + y + h / 2 - 2)))
  far=$(mean $((X + x + w + 66)) $((Y + y + h / 2 - 2)))
  python3 -c '
import sys
near = [int(v) for v in sys.argv[1].split()]
far = [int(v) for v in sys.argv[2].split()]
off = sum(abs(a - b) for a, b in zip(near, far))
sys.exit(0 if off <= 60 and max(near) >= 40 else 1)' "$near" "$far" ||
    ui_fail "beside $1 the screen is $near, the view's background $far: a dark frame round the card"
  echo "ok   beside $1 the screen shows the view's background ($near, $far)"
  checked=$((checked + 1))
}
beside browserDock
beside navigationBar
beside messageBandCard
[ "$checked" -ge 2 ] || ui_fail "fewer than two Floating cards to check"
ui_finish '3D mouse without a daemon'
