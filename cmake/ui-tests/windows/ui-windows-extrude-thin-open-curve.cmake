# SPDX-License-Identifier: MIT
# An open spline clicked in Extrude without changing Type (mitcad#2594):
# the click sets Type to Thin Extrude and takes the spline, and OK adds the
# wall along it. The Linux test's shared input script
# (tools/ui-extrude-thin-open-curve-test.sh).
add_test(NAME app.ui-windows-extrude-thin-open-curve
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-extrude-thin-open-curve-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-extrude-thin-open-curve"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-extrude-thin-open-curve PROPERTIES TIMEOUT 300)
