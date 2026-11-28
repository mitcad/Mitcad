# SPDX-License-Identifier: MIT
# The SOLID tab's MODIFY, SHEET PARTS, SURFACE, ASSEMBLE and CONSTRUCT
# menus in their sections with separators, and each group's own
# buttons, in both layouts (mitcad#1778); as
# tools/ui-solid-menus-test.sh on Linux.
add_test(NAME app.ui-windows-solid-menus
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-solid-menus-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-solid-menus"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-solid-menus PROPERTIES TIMEOUT 300)
