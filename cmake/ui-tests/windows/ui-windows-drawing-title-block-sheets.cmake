# SPDX-License-Identifier: MIT
# Delete Sheet while the title block is edited (mitcad#1841): another
# sheet deleted, Done writes the block onto the sheet edited; the sheet
# edited deleted ends the editor. The Linux test's shared input script
# (tools/ui-drawing-title-block-sheets-test.sh).
add_test(NAME app.ui-windows-drawing-title-block-sheets
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-title-block-sheets-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-title-block-sheets"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-title-block-sheets PROPERTIES TIMEOUT 180)
