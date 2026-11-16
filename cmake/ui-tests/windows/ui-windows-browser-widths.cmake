# SPDX-License-Identifier: MIT
# The Browser's width in the Floating layout (mitcad#1932): Rows widens to
# its names up to a share of the view, Card has a width grip whose width
# comes back after a restart. The Linux test's shared input scripts
# (tools/ui-browser-widths-test.sh).
add_test(NAME app.ui-windows-browser-widths
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-widths-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-widths"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browser-widths PROPERTIES TIMEOUT 420)
