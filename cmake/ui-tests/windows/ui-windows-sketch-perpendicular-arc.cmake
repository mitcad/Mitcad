# SPDX-License-Identifier: MIT
# Perpendicular of a line and an arc (mitcad#2114): Shift+R, the line,
# then the arc; the line turns through the arc's center and the symbol
# shows. The Linux test's shared input script
# (tools/ui-sketch-perpendicular-arc-test.sh).
add_test(NAME app.ui-windows-sketch-perpendicular-arc
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-perpendicular-arc-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-perpendicular-arc"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-perpendicular-arc PROPERTIES TIMEOUT 180)
