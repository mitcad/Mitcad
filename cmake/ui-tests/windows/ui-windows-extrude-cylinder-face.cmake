# SPDX-License-Identifier: MIT
# Extrude of a sketch on a cylinder's side with the automatic operation
# (mitcad#1908, GitHub mitcad/Mitcad#362): the picked profiles
# highlighted on the face, the distance arrow starting there, the
# automatic Join built and added. The Linux test's shared input script
# (tools/ui-extrude-cylinder-face-test.sh).
add_test(NAME app.ui-windows-extrude-cylinder-face
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-extrude-cylinder-face-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-extrude-cylinder-face"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-extrude-cylinder-face PROPERTIES TIMEOUT 300)
