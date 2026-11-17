# SPDX-License-Identifier: MIT
# A Form's opening (mitcad#1941): a face of a box cage deleted, Patch
# picking the Form body's opening by one free edge, and Fill Hole from Edit
# Form's context menu closing it again. The Linux test's shared input
# script (tools/ui-form-fill-hole-test.sh).
add_test(NAME app.ui-windows-form-fill-hole
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-form-fill-hole-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-form-fill-hole"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-form-fill-hole PROPERTIES TIMEOUT 360)
