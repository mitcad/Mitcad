# SPDX-License-Identifier: MIT
# Axis Perpendicular at Point on a plane (mitcad#2282): its Face field takes
# a construction plane and the origin plane XY, and the axis goes through
# the point's projection onto the plane, along its normal. The Linux test's
# shared input script (tools/ui-construct-axis-on-plane-test.sh).
add_test(NAME app.ui-windows-construct-axis-on-plane
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-construct-axis-on-plane-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-construct-axis-on-plane"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-construct-axis-on-plane PROPERTIES TIMEOUT 300)
