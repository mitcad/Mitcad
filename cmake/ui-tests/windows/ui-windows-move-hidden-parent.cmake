# SPDX-License-Identifier: MIT
# Move on a component hidden with its parent (mitcad#1995): on the
# reported design of the private test files (MITCAD_MITCAD_REPORTED);
# skipped without it. The Linux test's shared input script
# (tools/ui-move-hidden-parent-test.sh).
add_test(NAME app.ui-windows-move-hidden-parent
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-move-hidden-parent-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-move-hidden-parent"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-move-hidden-parent PROPERTIES
  TIMEOUT 400 SKIP_RETURN_CODE 77)
