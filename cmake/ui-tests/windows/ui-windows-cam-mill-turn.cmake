# SPDX-License-Identifier: MIT
# CAM mill-turn in the CNC group (mitcad#2067): a lathe setup with live
# tools on a C axis, turning and live tool operations on the face and the
# diameter, Generate, Simulate and Post Process... to the Fanuc turning
# dialect with the C axis. The Linux test's shared input script
# (tools/ui-cam-mill-turn-test.sh).
add_test(NAME app.ui-windows-cam-mill-turn
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cam-mill-turn-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-cam-mill-turn"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-cam-mill-turn PROPERTIES TIMEOUT 420)
