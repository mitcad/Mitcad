# SPDX-License-Identifier: MIT
# Pattern on Path along a body's edges with Tangent Chain (mitcad#2105,
# GitHub mitcad/Mitcad#322): one top edge of a block with two rounded
# vertical edges as the path, its tangent chain hovered and taken; the
# copies follow the rounded outline (checked by mitcad-cli). The Linux
# test's shared input scripts (tools/ui-path-pattern-edges-test.sh).
add_test(NAME app.ui-windows-path-pattern-edges
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-path-pattern-edges-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-path-pattern-edges"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-path-pattern-edges PROPERTIES TIMEOUT 600)
