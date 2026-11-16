# SPDX-License-Identifier: MIT
# Hidden built-ins (mitcad#1270): a built-in printer hidden in the
# Printers manager is gone from it and from Send to Slicer's Printer
# box, comes back grayed with Show hidden and Unhide restores it; a
# design that uses it keeps it; with a printer in My library Built-in
# starts folded. The Linux test is tools/ui-hide-built-ins-test.sh.
add_test(NAME app.ui-windows-hide-built-ins
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-hide-built-ins-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-hide-built-ins"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-hide-built-ins PROPERTIES TIMEOUT 300)
