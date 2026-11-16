# SPDX-License-Identifier: MIT
# The sketch's axes and origin as picks of Sketch Dimension and the
# constraint tools (mitcad#500): a distance from the x axis, an angle to
# it and a point on the y axis in a sketch with nothing at the origin.
add_test(NAME app.ui-windows-sketch-axes
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-axes-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-axes"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-axes PROPERTIES TIMEOUT 300)
