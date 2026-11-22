# SPDX-License-Identifier: MIT
# Insert Points (mitcad#2232): a NACA 2412 file written by the test from
# the 4-digit formula, inserted on a new design's XY plane as a fitted
# spline closed across its trailing edge, and its profile extruded. The
# Linux test's shared input script (tools/ui-points-test.sh).
add_test(NAME app.ui-windows-points
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-points-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-points"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-points PROPERTIES TIMEOUT 300)
