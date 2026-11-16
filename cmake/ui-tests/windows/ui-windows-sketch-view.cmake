# SPDX-License-Identifier: MIT
# Sketch mode's view: fully constrained lines recoloured when the
# background turns dark or light (mitcad#253), the sketch origin's place
# and the pointer's sketch coordinates (mitcad#254).
add_test(NAME app.ui-windows-sketch-view
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-view-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-view"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-view PROPERTIES TIMEOUT 420)
