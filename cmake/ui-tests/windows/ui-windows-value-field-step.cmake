# SPDX-License-Identifier: MIT
# Up, Down and the mouse wheel step a value field's number (GitHub
# mitcad/Mitcad#129, mitcad#1254): an expression left alone, Shift+Up
# by 10 mm, Circular Pattern's Quantity 4 to 5 previewed, the wheel only
# over the field with the keyboard; the Linux test's shared input script.
add_test(NAME app.ui-windows-value-field-step
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-value-field-step-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-value-field-step"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-value-field-step PROPERTIES TIMEOUT 240)
