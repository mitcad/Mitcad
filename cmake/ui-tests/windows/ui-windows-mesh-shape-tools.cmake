# SPDX-License-Identifier: MIT
# Mesh tools that change a mesh's shape (mitcad#2515): the shared steps of
# tools/ui-scripts/mesh-shape-tools.mitcad-ui, each panel (Smooth, Combine,
# Offset as a shell, Align, Plane Cut) on two boxes' and a cylinder's
# meshes (tools/ui-mesh-shape-tools-test.sh).
add_test(NAME app.ui-windows-mesh-shape-tools
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-mesh-shape-tools-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-mesh-shape-tools"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-mesh-shape-tools PROPERTIES TIMEOUT 600)
