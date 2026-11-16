# SPDX-License-Identifier: MIT
# The Sketch Palette's Show points (mitcad#1705): off, no point marker
# is drawn while a hidden end point still shows under the pointer, is
# dragged and a point selected; remembered for the next edit; on again,
# all drawn. The Linux test's shared input script
# (tools/ui-sketch-show-points-test.sh).
add_test(NAME app.ui-windows-sketch-show-points
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-show-points-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-show-points"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-show-points PROPERTIES TIMEOUT 300)
