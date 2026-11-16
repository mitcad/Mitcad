# SPDX-License-Identifier: MIT
# Convert Mesh (mitcad#1082): an inserted cube's mesh body made a
# faceted solid by the panel's defaults, the browser listing the solid.
add_test(NAME app.ui-windows-convert-mesh
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-convert-mesh-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-convert-mesh"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-convert-mesh PROPERTIES TIMEOUT 300)
