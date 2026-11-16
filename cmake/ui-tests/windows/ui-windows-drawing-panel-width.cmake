# SPDX-License-Identifier: MIT
# The drawing panel's width (mitcad#1692): its edge dragged toward the
# sheet widens it and a base view's Shows with it, Update from Model's
# text fits, and the width is kept when the drawing opens again and at
# the next start, Floating and Docked (tools/ui-drawing-panel-width-test.sh).
add_test(NAME app.ui-windows-drawing-panel-width
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-panel-width-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-panel-width"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-panel-width PROPERTIES TIMEOUT 300)
