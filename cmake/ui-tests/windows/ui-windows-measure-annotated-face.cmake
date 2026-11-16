# SPDX-License-Identifier: MIT
# Measure picks a face that carries an annotation, drawn opaque
# (mitcad#1166), as on Linux.
add_test(NAME app.ui-windows-measure-annotated-face
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-measure-annotated-face-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-measure-annotated-face"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-measure-annotated-face PROPERTIES TIMEOUT 180)
