# SPDX-License-Identifier: MIT
# Horizontal/Vertical in the other direction (mitcad#2184): two points
# nearer to level whose heights are held; Shift+V, the first point, the
# plumb glyph on the second, and the click adds a vertical alignment. The
# Linux test's shared input script
# (tools/ui-sketch-horizontal-vertical-other-test.sh).
add_test(NAME app.ui-windows-sketch-horizontal-vertical-other
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-horizontal-vertical-other-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-horizontal-vertical-other"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-horizontal-vertical-other PROPERTIES TIMEOUT 180)
