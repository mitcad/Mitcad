# SPDX-License-Identifier: MIT
# A BUILD profile started on a face (mitcad#2000): on a leg's bottom face
# Direction Face normal runs it down from the face's centre, its sides
# along the face's longest edge, Rotate offered; on the XY plane Direction
# -Z runs it down. The Linux test's shared input script
# (tools/ui-build-face-normal-test.sh).
add_test(NAME app.ui-windows-build-face-normal
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-face-normal-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-build-face-normal"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-build-face-normal PROPERTIES TIMEOUT 300)
