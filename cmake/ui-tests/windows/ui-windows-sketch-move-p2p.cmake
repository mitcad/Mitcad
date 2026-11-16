# SPDX-License-Identifier: MIT
# Sketch Move/Copy (mitcad#1563, GitHub mitcad/Mitcad#212): Point to
# Point from a rectangle's corner to a circle's center, Translate's
# handle dragged onto a point, and Rotate without a pivot about a
# line's middle. The Linux test's shared input script
# (tools/ui-sketch-move-p2p-test.sh).
add_test(NAME app.ui-windows-sketch-move-p2p
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-move-p2p-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-move-p2p"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-move-p2p PROPERTIES TIMEOUT 300)
