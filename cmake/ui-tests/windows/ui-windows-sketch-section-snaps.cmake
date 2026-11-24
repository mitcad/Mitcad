# SPDX-License-Identifier: MIT
# The drawing tools snap to the centres of circular body edges and to where
# the sketch plane cuts the bodies (mitcad#2324): a line to a cylinder's
# axis ends on its caps' projected centre, one to a sphere's centre on its
# linked section's centre, each one undo step; the end follows the moved
# sphere. The Linux test's shared input script
# (tools/ui-sketch-section-snaps-test.sh).
add_test(NAME app.ui-windows-sketch-section-snaps
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-section-snaps-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-section-snaps"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-section-snaps PROPERTIES TIMEOUT 180)
