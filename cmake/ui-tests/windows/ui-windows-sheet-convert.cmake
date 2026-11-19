# SPDX-License-Identifier: MIT
# Convert to Sheet Metal (mitcad#1949): a solid tray's base and its four
# bottom edges picked in the panel, OK, one sheet with its corners ripped;
# the conversion edited loads its picks. The Linux test's shared input
# script (tools/ui-sheet-convert-test.sh).
add_test(NAME app.ui-windows-sheet-convert
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-convert-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-convert"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-convert PROPERTIES TIMEOUT 240)
