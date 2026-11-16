# SPDX-License-Identifier: MIT
# Dimension spacing (mitcad#1173), the steps of
# tools/ui-drawing-dimension-spacing-test.sh: a dimension placed or
# dragged snaps to the spacing of the one before and its multiples, Alt
# places freely.
add_test(NAME app.ui-windows-drawing-spacing
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-spacing-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-spacing"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-spacing PROPERTIES TIMEOUT 300)
