# SPDX-License-Identifier: MIT
# Editing a component's features whose To Object or Start Object is a
# sketch point or line (mitcad#1599): an extrusion up to a point, a
# hole down to one and an extrusion from a line keep the loaded object
# in the profiles' occurrence and preview. The Linux test's shared
# input script (tools/ui-extrude-to-point-test.sh).
add_test(NAME app.ui-windows-extrude-to-point
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-extrude-to-point-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-extrude-to-point"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-extrude-to-point PROPERTIES TIMEOUT 300)
