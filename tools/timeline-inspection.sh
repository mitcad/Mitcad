#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Runs the portable timeline inspection (tools/ui-timeline-inspection-test.py)
# by hand on a private Linux display:
#   tools/timeline-inspection.sh <mitcad> <mitcad-cli> <output folder> [<Qt library folder>]
# The ctest app.ui-linux-timeline-inspection (cmake/SelectionUiTests.cmake)
# runs it in the checks; this is not one of check-all's tools/ui-*-test.sh,
# which run without arguments on the test library.
set -eu
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
exec xvfb-run -a -s '-screen 0 1600x1000x24' python3 \
  "$here/ui-timeline-inspection-test.py" "$@"
