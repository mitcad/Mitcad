# SPDX-License-Identifier: MIT
# Extend to Meet (mitcad#2178): two lines and a line and an arc extended to
# meet, Tab stepping the preview to the second result. The Linux test's
# shared input script (tools/ui-sketch-extend-to-meet-test.sh).
add_test(NAME app.ui-windows-sketch-extend-to-meet
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-extend-to-meet-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-extend-to-meet"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-extend-to-meet PROPERTIES TIMEOUT 300)
