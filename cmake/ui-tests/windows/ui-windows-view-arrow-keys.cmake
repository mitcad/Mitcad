# SPDX-License-Identifier: MIT
# The arrow keys turn the view while it has the keyboard, not from a
# value field (mitcad#1211); the Linux test's shared input script.
add_test(NAME app.ui-windows-view-arrow-keys
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-arrow-keys-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-view-arrow-keys"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-view-arrow-keys PROPERTIES TIMEOUT 180)
