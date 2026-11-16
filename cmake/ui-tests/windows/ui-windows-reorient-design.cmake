# SPDX-License-Identifier: MIT
# Reorient Design (mitcad#502): the whole design turned 90 degrees about
# X, previewed in its dialog, kept by OK and taken back by Undo.
add_test(NAME app.ui-windows-reorient-design
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-reorient-design-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-reorient-design"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-reorient-design PROPERTIES TIMEOUT 300)
