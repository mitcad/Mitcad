# SPDX-License-Identifier: MIT
# Filling a closed table (mitcad#1044): the Rows tab's Import CSV…
# through its preview, Add Option… on the closed table with rows, and a
# size on the Configuration bar that sets a parameter column (the test
# driver's script).
add_test(NAME app.ui-windows-configuration-rows
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-configuration-rows-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-configuration-rows" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-configuration-rows PROPERTIES TIMEOUT 360)
