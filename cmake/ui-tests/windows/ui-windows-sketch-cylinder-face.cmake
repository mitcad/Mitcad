# SPDX-License-Identifier: MIT
# A sketch on a cylinder's side (mitcad#1768): Create Sketch picks the
# side, a rectangle typed 30 by 20 lies 30 mm around the face and 20 mm
# along it, its profile is drawn on the face, Finish Sketch. The Linux
# test's shared input script (tools/ui-sketch-cylinder-face-test.sh).
add_test(NAME app.ui-windows-sketch-cylinder-face
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-cylinder-face-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-cylinder-face"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-cylinder-face PROPERTIES TIMEOUT 180)
