# SPDX-License-Identifier: MIT
# Standard boxes of board (mitcad#1939) (mitcad#1939): the Linux test's shared input script
# (tools/ui-board-box-test.sh, tools/ui-scripts/board-box.mitcad-ui).
add_test(NAME app.ui-windows-board-box
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-board-box-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-board-box"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-board-box PROPERTIES TIMEOUT 240)
