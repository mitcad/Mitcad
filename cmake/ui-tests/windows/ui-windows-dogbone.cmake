# SPDX-License-Identifier: MIT
# Dogbone (mitcad#1011): the command on a pocket's body picked in the
# browser, OK, four reliefs cut and Dogbone1 in the timeline.
add_test(NAME app.ui-windows-dogbone
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-dogbone-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-dogbone"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-dogbone PROPERTIES TIMEOUT 300)
