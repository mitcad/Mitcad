# SPDX-License-Identifier: MIT
# The Browser rows' Activate radio button and Show eye places (mitcad#2223):
# Classic by default, on Right one column at the visible right edge in the
# Floating layout (Rows and Card) and the Docked one, clicks there activating
# and toggling. The Linux test's shared input scripts
# (tools/ui-browser-control-places-test.sh).
add_test(NAME app.ui-windows-browser-control-places
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browser-control-places-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-browser-control-places"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browser-control-places PROPERTIES TIMEOUT 420)
