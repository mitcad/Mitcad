# SPDX-License-Identifier: MIT
# Boundary Fill (mitcad#2149): the shared steps of
# tools/ui-scripts/boundary-fill.mitcad-ui, four surfaces and two planes
# filled as a new body, a clicked cell cut from a block, Undo, and the
# first fill edited with its cell (tools/ui-boundary-fill-test.sh).
add_test(NAME app.ui-windows-boundary-fill
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-boundary-fill-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-boundary-fill"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-boundary-fill PROPERTIES TIMEOUT 600)
