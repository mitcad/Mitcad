# SPDX-License-Identifier: MIT
# The sketch's own origin and axes drawn in a new design whose Origin
# folder's light bulb is off, on XY, XZ and YZ and in an edited sketch,
# the light bulb untouched, the origin snapped to (mitcad#739).
add_test(NAME app.ui-windows-sketch-origin
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-origin-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-origin"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-origin PROPERTIES TIMEOUT 300)
