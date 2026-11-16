# SPDX-License-Identifier: MIT
# The Dimension tool's picks choosing the dimension, the Centerline of
# a curved slot and the Arc tool's snap on a plate like the report's
# (mitcad#1272).
add_test(NAME app.ui-windows-drawing-dimension-picks
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-dimension-picks-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-dimension-picks"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-dimension-picks PROPERTIES TIMEOUT 300)
