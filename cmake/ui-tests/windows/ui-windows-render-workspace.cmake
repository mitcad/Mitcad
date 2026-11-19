# SPDX-License-Identifier: MIT
# The Render workspace (mitcad#2013): its stages, the docked environment
# and lights, a shot saved and Render All Shots with 4 samples, in the
# Floating layout's toolbar panel; the Linux test's shared input script
# (tools/ui-render-workspace-test.sh).
add_test(NAME app.ui-windows-render-workspace
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-render-workspace-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-render-workspace"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-render-workspace PROPERTIES TIMEOUT 900)
