# SPDX-License-Identifier: MIT
# Redefine Sketch Plane (mitcad#787): from a sketch's browser row onto
# a face with an offset, undone; a face of a later feature refused.
add_test(NAME app.ui-windows-sketch-redefine-plane
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-redefine-plane-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-redefine-plane" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-redefine-plane PROPERTIES TIMEOUT 300)
