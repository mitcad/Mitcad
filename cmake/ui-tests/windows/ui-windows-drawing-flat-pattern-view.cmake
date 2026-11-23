# SPDX-License-Identifier: MIT
# A base view of a sheet-metal body's Flat Pattern placed from the Shows
# list, a dimension on its bend line and its bend notes turned off
# (mitcad#2280). The Linux test's shared input script
# (tools/ui-drawing-flat-pattern-view-test.sh).
add_test(NAME app.ui-windows-drawing-flat-pattern-view
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-flat-pattern-view-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-flat-pattern-view"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-flat-pattern-view PROPERTIES TIMEOUT 240)
