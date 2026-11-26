# SPDX-License-Identifier: MIT
# A design of the Projects panel dragged onto the 3D view (mitcad#2428),
# Floating and Docked: inserted linked at the point of the face under the
# drop (one undo step), again while Extrude is open (it waits, then Copy),
# and a design linking the open one and the open design itself refused.
# The Linux test's shared input script (tools/ui-project-design-drop-test.sh).
add_test(NAME app.ui-windows-project-design-drop
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-project-design-drop-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-project-design-drop"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-project-design-drop PROPERTIES TIMEOUT 600)
