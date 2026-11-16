# SPDX-License-Identifier: MIT
# Any command pinned to the navigation bar (mitcad#1589, GitHub
# mitcad/Mitcad#224): Edit Parameters by default, Measure pinned from
# its INSPECT row, run from the bar in BUILD, dragged first, read back
# after a restart and unpinned from its right-click, Floating and
# Docked (tools/ui-nav-bar-pins-test.sh on Linux).
add_test(NAME app.ui-windows-nav-bar-pins
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-nav-bar-pins-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-nav-bar-pins"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-nav-bar-pins PROPERTIES TIMEOUT 420)
