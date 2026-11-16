# SPDX-License-Identifier: MIT
# Delete on the Start page (mitcad#1313): with Sketch1 selected, the
# Start tab clears the selection, and Delete and Backspace on the page
# and on its New Design card delete nothing of the design behind it, in
# the Floating and the Docked layout. The Linux test is
# tools/ui-start-page-delete-test.sh.
add_test(NAME app.ui-windows-start-page-delete
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-start-page-delete-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-start-page-delete"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-start-page-delete PROPERTIES TIMEOUT 300)
