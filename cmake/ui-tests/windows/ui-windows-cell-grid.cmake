# SPDX-License-Identifier: MIT
# Cell Grid (mitcad#2238): the Linux test's shared input script
# (tools/ui-cell-grid-test.sh, tools/ui-scripts/cell-grid.mitcad-ui).
add_test(NAME app.ui-windows-cell-grid
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cell-grid-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-cell-grid"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-cell-grid PROPERTIES TIMEOUT 240)
