# SPDX-License-Identifier: MIT
# Sheet process profiles (mitcad#1939) (mitcad#1939): the Linux test's shared input script
# (tools/ui-sheet-process-test.sh, tools/ui-scripts/sheet-process.mitcad-ui).
add_test(NAME app.ui-windows-sheet-process
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sheet-process-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sheet-process"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sheet-process PROPERTIES TIMEOUT 240)
