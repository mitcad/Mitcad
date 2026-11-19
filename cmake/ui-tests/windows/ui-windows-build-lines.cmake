# SPDX-License-Identifier: MIT
# Profile along sketch lines (mitcad#1965): Method Along sketch lines, a
# sketch picked whole from the Browser, Place: four profiles mitered at the
# corners in one undo step, following a line moved in the sketch. The Linux
# test's shared input script (tools/ui-build-lines-test.sh).
add_test(NAME app.ui-windows-build-lines
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-lines-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-build-lines"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-build-lines PROPERTIES TIMEOUT 300)
