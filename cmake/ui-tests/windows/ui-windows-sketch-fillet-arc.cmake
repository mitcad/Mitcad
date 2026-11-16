# SPDX-License-Identifier: MIT
# Sketch Fillet at a corner of a line and an arc picked by its corner
# point (mitcad#1192); the Linux test's shared input script.
add_test(NAME app.ui-windows-sketch-fillet-arc
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-fillet-arc-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-fillet-arc"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-fillet-arc PROPERTIES TIMEOUT 300)
