# SPDX-License-Identifier: MIT
# Interference checks kept in the Analysis folder (mitcad#2664): Body1 and
# Body2 kept as Interference1, every body as Interference2, both run again
# from the browser after the geometry changes, saved, opened again, and
# Interference1 edited once Body2 is gone. The Linux test's shared input
# scripts (tools/ui-interference-check-test.sh).
add_test(NAME app.ui-windows-interference-check
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-interference-check-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-interference-check"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-interference-check PROPERTIES TIMEOUT 600)
