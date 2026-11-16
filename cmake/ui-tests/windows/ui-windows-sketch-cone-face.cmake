# SPDX-License-Identifier: MIT
# A sketch on a cone's side (mitcad#1853): Create Sketch picks the
# side, no layout grid on it, a rectangle typed 20 by 12 keeps its
# lengths on the face, its profile and its dimensions' lines are drawn
# on the face, Finish Sketch draws the grid again; the picture stays in
# files\. The Linux test's shared input script
# (tools/ui-sketch-cone-face-test.sh).
add_test(NAME app.ui-windows-sketch-cone-face
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-cone-face-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-cone-face"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-cone-face PROPERTIES TIMEOUT 180)
