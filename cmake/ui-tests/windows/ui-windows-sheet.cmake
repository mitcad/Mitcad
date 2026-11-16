# SPDX-License-Identifier: MIT
# Sheet metal (mitcad#210): Face, Flange, rule edits, Flat Pattern, its
# DXF export, an edit from the timeline, the Floating layout.
add_test(NAME app.ui-windows-sheet
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet PROPERTIES TIMEOUT 300)
