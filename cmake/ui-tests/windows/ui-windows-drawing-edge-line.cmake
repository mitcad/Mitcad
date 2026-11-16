# SPDX-License-Identifier: MIT
# Edit Line on a projected edge of a drawing's view sets the edge's
# width and pattern, and Reset to View Style takes them away
# (mitcad#1546). The Linux test's shared input script
# (tools/ui-drawing-edge-line-test.sh).
add_test(NAME app.ui-windows-drawing-edge-line
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-edge-line-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-edge-line"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-edge-line PROPERTIES TIMEOUT 180)
