# SPDX-License-Identifier: MIT
# Replace Component (mitcad#2640): a linked component replaced in all its
# places with its joint kept, OK and Undo; This one previewed and
# canceled. The Linux test's shared input script
# (tools/ui-replace-component-test.sh).
add_test(NAME app.ui-windows-replace-component
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-replace-component-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-replace-component"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-replace-component PROPERTIES TIMEOUT 240)
