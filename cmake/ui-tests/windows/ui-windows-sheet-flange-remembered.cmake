# SPDX-License-Identifier: MIT
# Flange's remembered choices (mitcad#1868, GitHub mitcad/Mitcad#348):
# a new Flange starts with the last OK's Measured and Position, an edit
# loads the flange's own. The Linux test's shared input script
# (tools/ui-sheet-flange-remembered-test.sh).
add_test(NAME app.ui-windows-sheet-flange-remembered
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-flange-remembered-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-flange-remembered"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-flange-remembered PROPERTIES TIMEOUT 240)
