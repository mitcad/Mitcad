# SPDX-License-Identifier: MIT
# The print managers (mitcad#1135): Printers and Processes apart, the
# groups by place, a project item edited for the project's other
# design, a built-in material's Duplicate asking where, no Tools › My
# Library.
add_test(NAME app.ui-windows-print-managers
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-print-managers-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-print-managers" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-print-managers PROPERTIES TIMEOUT 300)
