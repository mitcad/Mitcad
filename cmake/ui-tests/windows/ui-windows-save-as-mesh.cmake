# SPDX-License-Identifier: MIT
# Save As Mesh (mitcad#734): from a body's, a component's and the
# root's browser rows, 3MF on the print bed face, text STL in inches,
# OBJ a file per body with a longest edge; disabled for a surface body.
add_test(NAME app.ui-windows-save-as-mesh
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-save-as-mesh-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-save-as-mesh" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-save-as-mesh PROPERTIES TIMEOUT 300)
