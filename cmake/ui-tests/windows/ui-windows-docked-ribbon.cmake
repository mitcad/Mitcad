# SPDX-License-Identifier: MIT
# The ribbon, the Browser and the navigation bar with the design in
# front (mitcad#1451, GitHub mitcad/Mitcad#185): a member placed with
# Build > Profile, a second design and back, Docked and Floating; the
# window's menu of its panels offers no Toolbar. The Linux test's shared
# input script (tools/ui-docked-ribbon-test.sh).
add_test(NAME app.ui-windows-docked-ribbon
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-docked-ribbon-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-docked-ribbon"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-docked-ribbon PROPERTIES TIMEOUT 600)
