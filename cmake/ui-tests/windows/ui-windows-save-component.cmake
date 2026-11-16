# SPDX-License-Identifier: MIT
# Save Component As Design (mitcad#1057): the entry on the rows of the
# design's own components only (not the root's, not a linked one's),
# the dialog listing a projection of another component as frozen, the
# file written under the component's name, and Replace with a link.
add_test(NAME app.ui-windows-save-component
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-save-component-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-save-component" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-save-component PROPERTIES TIMEOUT 300)
