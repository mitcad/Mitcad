# SPDX-License-Identifier: MIT
# The window's accessibility tree small and quick to read, and the log line
# that names the process reading it through UI Automation (mitcad#2227).
add_test(NAME app.ui-windows-accessible-tree
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-accessible-tree-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -UiaWalk "$<TARGET_FILE:mitcad-uia-walk>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-accessible-tree"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-accessible-tree PROPERTIES TIMEOUT 300)
