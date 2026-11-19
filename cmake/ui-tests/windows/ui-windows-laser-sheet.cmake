# SPDX-License-Identifier: MIT
# Laser sheet joints (mitcad#1939) (mitcad#1939): the Linux test's shared input script
# (tools/ui-laser-sheet-test.sh, tools/ui-scripts/laser-sheet.mitcad-ui).
add_test(NAME app.ui-windows-laser-sheet
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-laser-sheet-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-laser-sheet"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-laser-sheet PROPERTIES TIMEOUT 240)
