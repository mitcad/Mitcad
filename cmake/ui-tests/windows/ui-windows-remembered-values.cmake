# SPDX-License-Identifier: MIT
# Remembered values (mitcad#1028): a fillet and a chamfer with 3 mm,
# OK; Fillet and Chamfer again start with 3 mm, also after a new start;
# Cancel changes nothing. And Thread's settings kept by size
# (mitcad#1298): two modelled threads, so a longer timeout.
add_test(NAME app.ui-windows-remembered-values
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-remembered-values-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-remembered-values"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-remembered-values PROPERTIES TIMEOUT 900)
