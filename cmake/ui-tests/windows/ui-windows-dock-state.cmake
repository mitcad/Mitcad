# SPDX-License-Identifier: MIT
# The Docked layout's docks kept between starts (mitcad#2070, GitHub
# mitcad/Mitcad#451): moved, floated and shown, the same at the next start
# behind and after the Start page, the Command width from the kept
# arrangement, the Render workspace's docks (its workspace selector, mitcad#2082),
# a floating dock whose screen is gone docked, a new window
# with nothing floating, Restore Default Layout. The Linux test's shared
# input scripts (tools/ui-dock-state-test.sh).
add_test(NAME app.ui-windows-dock-state
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-dock-state-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-dock-state"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-dock-state PROPERTIES TIMEOUT 600)
