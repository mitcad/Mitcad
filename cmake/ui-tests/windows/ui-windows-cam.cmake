# SPDX-License-Identifier: MIT
# CAM's CNC group (mitcad#1013): a setup and four operations from their
# panels, Generate, out of date, Simulate, Post Process… to grbl,
# Machines and Tools… and the Browser's CAM rows. The Linux test's
# shared input script (tools/ui-cam-test.sh).
add_test(NAME app.ui-windows-cam
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cam-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-cam"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-cam PROPERTIES TIMEOUT 420)
