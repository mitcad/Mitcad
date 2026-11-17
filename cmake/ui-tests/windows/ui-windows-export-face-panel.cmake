# SPDX-License-Identifier: MIT
# Export Face as DXF... with nothing selected (mitcad#1963): its panel with a
# Face input opens, and the face picked there is written.
add_test(NAME app.ui-windows-export-face-panel
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-export-face-panel-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-export-face-panel"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-export-face-panel PROPERTIES TIMEOUT 300)
