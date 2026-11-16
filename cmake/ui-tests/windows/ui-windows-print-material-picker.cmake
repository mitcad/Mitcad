# SPDX-License-Identifier: MIT
# Send to Slicer's material rows (mitcad#1141): a body without a
# material says so, a material chosen in the row's picker is the
# body's at once (one undo step), a row per filament changed alone,
# Edit materials… opens the Materials window on the row's material.
add_test(NAME app.ui-windows-print-material-picker
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-material-picker-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-print-material-picker"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-print-material-picker PROPERTIES TIMEOUT 300)
