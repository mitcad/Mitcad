# SPDX-License-Identifier: MIT
# A sketch on a planar face starts with the face's outline, and Convert to
# Sketch Geometry takes a profile (mitcad#2051). The Linux test's shared
# input script (tools/ui-sketch-face-outline-test.sh).
add_test(NAME app.ui-windows-sketch-face-outline
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-face-outline-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-face-outline"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-face-outline PROPERTIES TIMEOUT 240)
