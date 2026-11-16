# SPDX-License-Identifier: MIT
# A failed preview's message opens at its first line (mitcad#1430): a
# cube's Extrude edited with its distance 0 in a 1280 x 800 window,
# Docked and Floating, the panel's error box not scrolled down its text.
# The Linux test's shared input script
# (tools/ui-panel-message-top-test.sh).
add_test(NAME app.ui-windows-panel-message-top
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-panel-message-top-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-panel-message-top"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-panel-message-top PROPERTIES TIMEOUT 300)
