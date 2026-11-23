# SPDX-License-Identifier: MIT
# Lines started and ended at the ends of a derived offset of a spline are
# held there as the source moves (mitcad#2278): the Line tool closes a
# profile of a spline, its offset and two lines, and a drag of the spline's
# end keeps it closed; the Linux test's shared input script.
add_test(NAME app.ui-windows-sketch-offset-points
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-offset-points-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-offset-points"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-offset-points PROPERTIES TIMEOUT 300)
