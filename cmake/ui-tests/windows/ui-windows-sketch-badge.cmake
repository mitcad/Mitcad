# SPDX-License-Identifier: MIT
# Sketch mode's badge and the pointer's place clear of the floating
# cards, its English plural, and the annotations rolled back while a
# sketch is edited (mitcad#1756); tools/ui-sketch-badge-test.sh on Linux.
add_test(NAME app.ui-windows-sketch-badge
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-badge-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-badge"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-badge PROPERTIES TIMEOUT 300)
