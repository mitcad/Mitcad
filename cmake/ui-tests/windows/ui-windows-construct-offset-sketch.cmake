# SPDX-License-Identifier: MIT
# Offset Plane takes a sketch (mitcad#2167): the sketch picked in the
# Browser and its profile clicked in the view are each taken as the
# sketch's plane, OK makes the plane. The Linux test's shared input script
# (tools/ui-construct-offset-sketch-test.sh).
add_test(NAME app.ui-windows-construct-offset-sketch
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-construct-offset-sketch-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-construct-offset-sketch"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-construct-offset-sketch PROPERTIES TIMEOUT 300)
