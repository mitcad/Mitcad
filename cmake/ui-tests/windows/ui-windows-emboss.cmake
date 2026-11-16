# SPDX-License-Identifier: MIT
# Emboss by projection (mitcad#1091): two letters of a text projected
# onto a ball and raised, previewed and made; a rectangle wrapped onto a
# rod moved by its Along Axis arrow and by Along Axis, Around and
# Rotation typed (mitcad#1259).
add_test(NAME app.ui-windows-emboss
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-emboss-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-emboss"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-emboss PROPERTIES TIMEOUT 420)
