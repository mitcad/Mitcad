#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MeshToolsCommands.cpp app/commands/ModifyCommands.cpp
# check-all sources: core/model/src/mesh_tools core/model/src/features/mesh_tools.rs
# check-all sources: core/model/src/document/mesh_sections.rs core/model-api/src/mesh_tools.rs tools/cli
# check-all sources: tools/ui-scripts/mesh-tools.mitcad-ui tools/ui-scripts/mesh-tools.json
# Mesh tools for reverse engineering (mitcad#1871) through the real UI, the
# shared steps tools/ui-scripts/mesh-tools.mitcad-ui (also run by
# tools/ui-windows-mesh-tools-test.ps1 and tools/ui-macos-mesh-tools-test.sh)
# on the design mitcad-cli makes (tools/ui-scripts/mesh-tools.json, run in
# the work folder, where it writes its STL file): each panel, Mesh Section,
# Repair Mesh, Decimate Mesh, Remove Region by a lasso, Segment Mesh and
# Fit Surface, on a cylinder's mesh.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-mesh-tools-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-mesh-tools.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/mesh-tools.mitcad-ui
(cd "$work" && "$cli" run "$here/ui-scripts/mesh-tools.json" --save "$work/mesh-tools.mitcad") || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/mesh-tools.mitcad"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'mesh tools driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'each mesh tool panel made its body'
ui_finish 'UI mesh tools'
