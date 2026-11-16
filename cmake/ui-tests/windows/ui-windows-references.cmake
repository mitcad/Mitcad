# SPDX-License-Identifier: MIT
# An .f3d design of components of other documents (mitcad#196): the
# file dialog for each document not found, skipped or chosen.
add_test(NAME app.ui-windows-references
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-references-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-references"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-references PROPERTIES TIMEOUT 300)
