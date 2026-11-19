# SPDX-License-Identifier: MIT
# Mesh tools for reverse engineering (mitcad#1871): the shared steps of
# tools/ui-scripts/mesh-tools.mitcad-ui, each panel (Mesh Section, Repair,
# Decimate, Remove Region by a lasso, Segment, Fit Surface) on a cylinder's
# mesh (tools/ui-mesh-tools-test.sh).
add_test(NAME app.ui-windows-mesh-tools
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-mesh-tools-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-mesh-tools"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-mesh-tools PROPERTIES TIMEOUT 600)
