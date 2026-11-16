# SPDX-License-Identifier: MIT
# The view's lines (mitcad#1633): a body's round edges from fine
# detached copies of its edges, an edge's highlight on the body's
# polyline, a sketch's selected line and arc 200 mm from the origin on
# their curves zoomed in. The Linux test's shared input script
# (tools/ui-edge-mesh-test.sh).
add_test(NAME app.ui-windows-edge-mesh
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-edge-mesh-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-edge-mesh"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-edge-mesh PROPERTIES TIMEOUT 420)
