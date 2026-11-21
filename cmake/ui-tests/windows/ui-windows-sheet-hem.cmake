# SPDX-License-Identifier: MIT
# The Hem command (mitcad#2112): a Flat hem made, changed to Open, Rolled
# and Double by Edit Feature, unfolded and refolded. The Linux test's
# shared input script (tools/ui-sheet-hem-test.sh).
add_test(NAME app.ui-windows-sheet-hem
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-hem-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-hem"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-hem PROPERTIES TIMEOUT 300)
