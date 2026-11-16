# SPDX-License-Identifier: MIT
# The quick-access group (mitcad#1090): the default commands in the
# Docked toolbar and the Floating title row, Save disabled for an
# unchanged saved design and enabled after an edit, View > Look At
# added from the group's menu, read back, Reset to Default.
add_test(NAME app.ui-windows-quick-access
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-quick-access-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-quick-access" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-quick-access PROPERTIES TIMEOUT 300)
