# SPDX-License-Identifier: MIT
# Processes grouped by printer (mitcad#1216): the Processes manager's
# sub-groups, Send to Slicer's Process picker with the chosen printer's
# processes first, the body card's drop-down; the Linux test's shared
# input script.
add_test(NAME app.ui-windows-processes-by-printer
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-processes-by-printer-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-processes-by-printer"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-processes-by-printer PROPERTIES TIMEOUT 360)
