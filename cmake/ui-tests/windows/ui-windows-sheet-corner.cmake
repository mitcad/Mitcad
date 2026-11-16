# SPDX-License-Identifier: MIT
# The Flange panel's Corner (mitcad#1410): Open loaded and kept, Half
# Open chosen, saved and read back, None showing Measured again. The
# Linux test is tools/ui-sheet-flange-corner-test.sh.
add_test(NAME app.ui-windows-sheet-corner
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-corner-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-corner" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-corner PROPERTIES TIMEOUT 300)
