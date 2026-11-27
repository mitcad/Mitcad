#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MeshShapeCommands.cpp app/commands/MeshToolsSupport.hpp
# check-all sources: app/commands/ModifyCommands.cpp core/model/src/mesh_tools
# check-all sources: core/model/src/features/mesh_shape_tools.rs core/model-api/src/mesh_tools.rs
# check-all sources: tools/ui-scripts/mesh-shape-tools.mitcad-ui tools/ui-scripts/mesh-shape-tools.json
# Mesh tools that change a mesh's shape (mitcad#2515) through the real UI,
# the shared steps tools/ui-scripts/mesh-shape-tools.mitcad-ui (also run by
# tools/ui-windows-mesh-shape-tools-test.ps1 and
# tools/ui-macos-mesh-shape-tools-test.sh) on the design mitcad-cli makes
# (tools/ui-scripts/mesh-shape-tools.json, run in the work folder, where it
# writes its STL files): each panel, Smooth Mesh, Combine Meshes, Offset
# Mesh as a shell, Align Mesh and Plane Cut, on two boxes' and a cylinder's
# meshes.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-mesh-shape-tools-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-mesh-shape-tools.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/mesh-shape-tools.mitcad-ui
(cd "$work" && "$cli" run "$here/ui-scripts/mesh-shape-tools.json" --save "$work/mesh-shape-tools.mitcad") || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/mesh-shape-tools.mitcad"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'mesh shape tools driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'each mesh shape tool panel made its bodies'
ui_finish 'UI mesh shape tools'
