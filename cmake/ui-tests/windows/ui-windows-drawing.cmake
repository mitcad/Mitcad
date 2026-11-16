# SPDX-License-Identifier: MIT
# File > Export Drawing (mitcad#471): the dialog with its defaults, a
# typed SVG file, A3 chosen, the drawing written (3 views, 2 bodies,
# A3, 1:1), the choices kept.
add_test(NAME app.ui-windows-drawing
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing PROPERTIES TIMEOUT 300)
