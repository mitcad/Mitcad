# SPDX-License-Identifier: MIT
# The overlay layer's fallback (mitcad#2406): with the layer's failure
# forced (MITCAD_TEST_PAINT_LAYER_FAILURE=gl-error), sketch mode's overlay
# drawn as a widget over the view for the session, the log saying why; the
# widget drawing the sketch and a line drawn with the Line tool. The Linux
# test's shared input script (tools/ui-paint-layer-fallback-test.sh).
add_test(NAME app.ui-windows-paint-layer-fallback
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-paint-layer-fallback-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-paint-layer-fallback"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-paint-layer-fallback PROPERTIES TIMEOUT 200)
