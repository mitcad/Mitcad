# SPDX-License-Identifier: MIT
# The Thread panel suggests a size from a picked face only while none
# is chosen: not in an edit, nor after a size chosen by hand, and then
# from the whole size table (mitcad#1781). The Linux test's shared
# input script (tools/ui-thread-size-test.sh).
add_test(NAME app.ui-windows-thread-size
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-thread-size-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-thread-size"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-thread-size PROPERTIES TIMEOUT 240)
