# SPDX-License-Identifier: MIT
# Blank chosen in Sheet... (mitcad#2325): the title block's fields hidden,
# the sheet without frame and title block; ISO again brings them back with
# the title kept. The Linux test's shared input script
# (tools/ui-drawing-blank-sheet-test.sh).
add_test(NAME app.ui-windows-drawing-blank-sheet
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-blank-sheet-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-blank-sheet"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-blank-sheet PROPERTIES TIMEOUT 180)
