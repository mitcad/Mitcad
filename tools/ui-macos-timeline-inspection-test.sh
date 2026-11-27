#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/OcctViewer.cpp app/framework/TestDriver tools/cli
set -eu
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
ui_init "$1" "${3:-ui-macos-timeline-inspection}"
python3 "$here/ui-timeline-inspection-test.py" "$1" "$2" "$UI_OUT"
