# SPDX-License-Identifier: MIT
# The face's edges do not split a shape the user closed (mitcad#2197). The
# Linux test's shared input script (tools/ui-sketch-own-area-test.sh).
add_test(NAME app.ui-windows-sketch-own-area
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-own-area-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-own-area"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-own-area PROPERTIES TIMEOUT 240)
