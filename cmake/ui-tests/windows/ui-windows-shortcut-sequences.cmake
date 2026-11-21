# SPDX-License-Identifier: MIT
# Key sequences in Keyboard Shortcuts and the Keyboard and Mouse Overview's
# cheat sheet (mitcad#2095): the Linux test's shared input script.
add_test(NAME app.ui-windows-shortcut-sequences
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-shortcut-sequences-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-shortcut-sequences"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-shortcut-sequences PROPERTIES TIMEOUT 300)
