# SPDX-License-Identifier: MIT
# Help > User Guide (mitcad#1032), Docked and Floating: the menu entry
# and the command each would open the guide (the test driver's script,
# shared with the Linux test).
add_test(NAME app.ui-windows-user-guide
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-user-guide-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-user-guide" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-user-guide PROPERTIES TIMEOUT 300)
