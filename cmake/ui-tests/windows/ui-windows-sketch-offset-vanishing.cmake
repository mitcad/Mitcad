# SPDX-License-Identifier: MIT
# Sketch Offset of a hexagon whose 2 mm corner arcs vanish at 2 mm
# inward (mitcad#1234): the preview and OK without an error, the note
# naming the arcs; the Linux test's shared input script.
add_test(NAME app.ui-windows-sketch-offset-vanishing
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-offset-vanishing-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-offset-vanishing"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-offset-vanishing PROPERTIES TIMEOUT 300)
