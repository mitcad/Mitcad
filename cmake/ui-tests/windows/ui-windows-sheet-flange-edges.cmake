# SPDX-License-Identifier: MIT
# A Flange on several edges (mitcad#1872): three side faces picked
# into one Flange, Corner Gap and Position set, OK, the flange edited
# again loading its edges and gap. The Linux test's shared input
# script (tools/ui-sheet-flange-edges-test.sh).
add_test(NAME app.ui-windows-sheet-flange-edges
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-flange-edges-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-flange-edges"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-flange-edges PROPERTIES TIMEOUT 300)
