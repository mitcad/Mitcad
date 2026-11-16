# SPDX-License-Identifier: MIT
# A panel's new error scrolled into view by the panel itself
# (mitcad#1884): a cube's Extrude edited with its Distance 0 in a
# 1280 x 600 window, Docked and Floating, the error box and the field
# typed in within the page's view. The Linux test's shared input script
# (tools/ui-panel-message-reveal-test.sh).
add_test(NAME app.ui-windows-panel-message-reveal
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-panel-message-reveal-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-panel-message-reveal"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-panel-message-reveal PROPERTIES TIMEOUT 300)
