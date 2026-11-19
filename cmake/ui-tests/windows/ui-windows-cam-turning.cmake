# SPDX-License-Identifier: MIT
# CAM turning in the CNC group (mitcad#1869): a lathe setup, five turning
# operations from their panels, Generate, Simulate and Post Process… to the
# Fanuc turning dialect. The Linux test's shared input script
# (tools/ui-cam-turning-test.sh).
add_test(NAME app.ui-windows-cam-turning
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cam-turning-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-cam-turning"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-cam-turning PROPERTIES TIMEOUT 420)
