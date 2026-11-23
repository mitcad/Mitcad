# SPDX-License-Identifier: MIT
# Macros and scripts (mitcad#2245): Record Macro of a sketch, an extrusion
# and a fillet, Save Macro, Run Script as one undo step, File > Export
# Timeline as Script; the Linux test's shared input script
# (tools/ui-macro-record-test.sh).
add_test(NAME app.ui-windows-macro-record
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-macro-record-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-macro-record"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-macro-record PROPERTIES TIMEOUT 300)
