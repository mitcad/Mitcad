# SPDX-License-Identifier: MIT
# The scenario of GitHub mitcad/Mitcad#202 end to end (mitcad#1550): a
# rectangle sketched on a face, 3D Sketch checked, a line and a tangent
# arc onto a vertex of another component, a Pipe along them. The Linux
# test is tools/ui-sketch3d-202-test.sh.
add_test(NAME app.ui-windows-sketch3d-202
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch3d-202-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch3d-202"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch3d-202 PROPERTIES TIMEOUT 300)
