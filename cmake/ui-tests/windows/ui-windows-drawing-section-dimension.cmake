# SPDX-License-Identifier: MIT
# A section's cut edge dimensioned alone (mitcad#1361): the piece beside
# a hole picked alone reads its own length, a corner waits for a second
# pick.
add_test(NAME app.ui-windows-drawing-section-dimension
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-section-dimension-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-section-dimension"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-section-dimension PROPERTIES TIMEOUT 300)
