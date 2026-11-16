# SPDX-License-Identifier: MIT
# The gap between two concentric circles from Sketch Dimension
# (mitcad#445).
add_test(NAME app.ui-windows-concentric-dimension
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-concentric-dimension-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-concentric-dimension"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-concentric-dimension PROPERTIES TIMEOUT 300)
