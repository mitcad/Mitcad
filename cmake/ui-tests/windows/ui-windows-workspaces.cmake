# SPDX-License-Identifier: MIT
# The toolbar panel's workspace selector (mitcad#1990): no menu bar,
# Design's tabs, the Start page from Home without the panel, Design ->
# Make -> Render -> Drawing -> Design from the selector's list, pictures of
# the Start page, the Solid tab, the Make, the Render and the Drawing
# workspaces at
# 1024 x 768. The Linux test's shared input script
# (tools/ui-workspaces-test.sh).
add_test(NAME app.ui-windows-workspaces
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-workspaces-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-workspaces"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-workspaces PROPERTIES TIMEOUT 240)
