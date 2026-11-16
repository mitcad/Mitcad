# SPDX-License-Identifier: MIT
# Preferences > Display > Small circles (mitcad#879): Adaptive by
# default; Full Detail kept in the settings and the open design's body
# meshed again with more triangles; back to Adaptive, fewer. The Linux
# test is tools/ui-small-circles-test.sh.
add_test(NAME app.ui-windows-small-circles
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-small-circles-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-small-circles"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-small-circles PROPERTIES TIMEOUT 300)
