# SPDX-License-Identifier: MIT
# The view cube in a sketch with a constraint selected (mitcad#773):
# an active, an inactive and a drawn line's inferred constraint
# selected in turn, every arrow and Home clicked; the app lives on,
# writes no crash report and logs no failed view operation.
add_test(NAME app.ui-windows-sketch-cube
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-cube-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-cube"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-cube PROPERTIES TIMEOUT 300)
