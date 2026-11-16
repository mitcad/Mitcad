# SPDX-License-Identifier: MIT
# Move/Copy in the context menus (mitcad#735): on a body's row and on
# two occurrences' rows in the browser, and at the top of the view's
# menu for a body selected in the browser; none for a face alone.
add_test(NAME app.ui-windows-move-copy-menu
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-move-copy-menu-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-move-copy-menu" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-move-copy-menu PROPERTIES TIMEOUT 300)
