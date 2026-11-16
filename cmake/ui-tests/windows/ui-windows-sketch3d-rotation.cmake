# SPDX-License-Identifier: MIT
# The free rotation of a 3D sketch's placement plane (mitcad#1661): a
# route drawn with the rotated plane (Rotate typed, a handle dragged),
# Reset, Show Plane and Show Handles. The Linux test's shared input
# script (tools/ui-sketch3d-rotation-test.sh).
add_test(NAME app.ui-windows-sketch3d-rotation
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch3d-rotation-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch3d-rotation"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch3d-rotation PROPERTIES TIMEOUT 300)
