# SPDX-License-Identifier: MIT
# Move/Copy, Move Object Components, on a component three levels deep
# picked by its Browser row with the root active (mitcad#2302): the move
# goes into the component it is placed in. The Linux test's shared input
# script (tools/ui-move-nested-component-test.sh).
add_test(NAME app.ui-windows-move-nested-component
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-move-nested-component-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-move-nested-component"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-move-nested-component PROPERTIES TIMEOUT 240)
