# SPDX-License-Identifier: MIT
# Break and Split at Point (mitcad#2108): an arc split at a point, a line
# broken at its crossings with a circle, the pieces held together. The
# Linux test's shared input script (tools/ui-sketch-break-test.sh).
add_test(NAME app.ui-windows-sketch-break
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-break-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-break"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-break PROPERTIES TIMEOUT 300)
