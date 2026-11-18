# SPDX-License-Identifier: MIT
# Patch takes a sketch curve for its boundary from the Browser
# (mitcad#1978). On the reported design of the private test files
# (MITCAD_MITCAD_REPORTED); skipped without it. The Linux test's shared
# input script (tools/ui-surface-patch-browser-curve-test.sh).
add_test(NAME app.ui-windows-surface-patch-browser-curve
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-surface-patch-browser-curve-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-surface-patch-browser-curve"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-surface-patch-browser-curve PROPERTIES
  TIMEOUT 1000 SKIP_RETURN_CODE 77)
