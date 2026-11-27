# SPDX-License-Identifier: MIT
# Perspective with Orthographic Faces (mitcad#2473): a cube face click logs
# an orthographic camera, an orbit a perspective one, the cube's edge and
# corner stay perspective. The Linux test's shared input script
# (tools/ui-view-orthographic-faces-test.sh).
add_test(NAME app.ui-windows-view-orthographic-faces
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-orthographic-faces-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-view-orthographic-faces"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-view-orthographic-faces PROPERTIES TIMEOUT 300)
