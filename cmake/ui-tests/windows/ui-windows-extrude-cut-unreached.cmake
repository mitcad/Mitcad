# SPDX-License-Identifier: MIT
# An Extrude Cut whose preview fails before its cut's targets are found
# (mitcad#2118, GitHub mitcad/Mitcad#492): the Affected components line
# gives the failure in words, not "Updating affected components...", and the
# panel's error is in the page's view after the list comes back above it,
# Docked and Floating. The Linux test's shared input script
# (tools/ui-extrude-cut-unreached-test.sh).
add_test(NAME app.ui-windows-extrude-cut-unreached
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-extrude-cut-unreached-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-extrude-cut-unreached"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-extrude-cut-unreached PROPERTIES TIMEOUT 300)
