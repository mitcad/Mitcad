# SPDX-License-Identifier: MIT
# The Browser's component color bars (mitcad#1876): every component,
# occurrence and body row in its component's timeline color, the root's
# orange, following a rename, a move and a new component. The Linux
# test's shared input script (tools/ui-browser-component-colors-test.sh).
add_test(NAME app.ui-windows-browser-component-colors
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-component-colors-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-component-colors"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browser-component-colors PROPERTIES TIMEOUT 240)
