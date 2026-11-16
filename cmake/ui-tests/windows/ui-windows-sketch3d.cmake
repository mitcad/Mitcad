# SPDX-License-Identifier: MIT
# 3D sketches (mitcad#1524): Create Sketch's 2D or 3D choice, an empty
# new 3D sketch taken back, an API-made 3D sketch on a box's face
# edited: its frame, a point picked by its id and described, the 2D
# tools disabled, Esc. The Linux test is tools/ui-sketch3d-test.sh.
add_test(NAME app.ui-windows-sketch3d
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch3d-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch3d"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch3d PROPERTIES TIMEOUT 300)
