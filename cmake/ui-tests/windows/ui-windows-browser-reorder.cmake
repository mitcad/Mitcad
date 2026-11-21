# SPDX-License-Identifier: MIT
# The order of components in the Browser (mitcad#2104): a drag before
# another row, Move Down, Undo, a refused drop, kept with the design. The
# Linux test's shared input script (tools/ui-browser-reorder-test.sh).
add_test(NAME app.ui-windows-browser-reorder
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-reorder-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-reorder"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browser-reorder PROPERTIES TIMEOUT 300)
