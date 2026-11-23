# SPDX-License-Identifier: MIT
# Edit Parameters' Expression cells' fx (mitcad#2290): entering a cell
# opens no function list; fx opens Edit Formula with the text typed so far
# on an existing row and on the empty row, whose OK makes the parameter;
# the bottom Edit Formula… uses an edited cell's text and works on the
# empty row. The Linux test's shared input script
# (tools/ui-parameters-fx-test.sh).
add_test(NAME app.ui-windows-parameters-fx
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-parameters-fx-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-parameters-fx"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-parameters-fx PROPERTIES TIMEOUT 300)
