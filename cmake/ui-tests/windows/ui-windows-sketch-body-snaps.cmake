# SPDX-License-Identifier: MIT
# Every drawing tool snaps to body corners and edges in the sketch plane
# (mitcad#1967): a line and a rectangle from block corners, a line from a
# block edge, each held on the linked projection. The Linux test's shared
# input script (tools/ui-sketch-body-snaps-test.sh).
add_test(NAME app.ui-windows-sketch-body-snaps
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-body-snaps-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-body-snaps"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-body-snaps PROPERTIES TIMEOUT 180)
