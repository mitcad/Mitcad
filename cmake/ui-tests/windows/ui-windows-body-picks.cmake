# SPDX-License-Identifier: MIT
# Body edges and vertices in the sketch plane (mitcad#256, mitcad#258):
# Sketch Dimension from a block edge in one undo step and its projection
# reused; the Point tool on a corner and at an edge's middle, the points
# following the block.
add_test(NAME app.ui-windows-body-picks
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-body-picks-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-body-picks"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-body-picks PROPERTIES TIMEOUT 300)
