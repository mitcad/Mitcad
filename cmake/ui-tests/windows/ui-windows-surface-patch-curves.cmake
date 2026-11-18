# SPDX-License-Identifier: MIT
# Patch closes part of an opening with two single free edges and a sketch
# line (mitcad#1972, GitHub mitcad/Mitcad#407). On the reported design of
# the private test files (MITCAD_MITCAD_REPORTED); skipped without it. The
# Linux test's shared input script (tools/ui-surface-patch-curves-test.sh).
add_test(NAME app.ui-windows-surface-patch-curves
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-surface-patch-curves-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-surface-patch-curves"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-surface-patch-curves PROPERTIES
  TIMEOUT 1000 SKIP_RETURN_CODE 77)
