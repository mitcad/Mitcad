# SPDX-License-Identifier: MIT
# Placements held where the snap marker showed (mitcad#1645): a paste,
# Move/Copy's handle and a Point to Point copy dropped on points, a
# text's frame and arcs ending on points. The Linux test's shared input
# script (tools/ui-sketch-snap-hold-test.sh). Its Copy writes the system
# clipboard (app.ui-windows-sketch-clipboard).
add_test(NAME app.ui-windows-sketch-snap-hold
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-snap-hold-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-snap-hold"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-snap-hold PROPERTIES TIMEOUT 360 RESOURCE_LOCK system_clipboard)
