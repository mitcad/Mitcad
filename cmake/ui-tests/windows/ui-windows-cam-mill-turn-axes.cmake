# SPDX-License-Identifier: MIT
# CAM mill-turn with a Y axis, a B head and a sub-spindle (mitcad#2319): a
# lathe setup handing over to the sub-spindle, a flat with the Y axis, a
# hole on a tilted plane, a sketch wrapped onto the diameter, Generate,
# Simulate (the stock in 3D) and Post Process... to the Fanuc turning
# dialect. The Linux test's shared input script
# (tools/ui-cam-mill-turn-axes-test.sh).
add_test(NAME app.ui-windows-cam-mill-turn-axes
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cam-mill-turn-axes-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-cam-mill-turn-axes"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-cam-mill-turn-axes PROPERTIES TIMEOUT 500)
