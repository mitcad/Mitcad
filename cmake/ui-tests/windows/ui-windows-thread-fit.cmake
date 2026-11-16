# SPDX-License-Identifier: MIT
# The Thread panel fits a hole too small for the thread (mitcad#1632):
# its note's button, the thread and the fit one undo step; the Linux
# test's shared input script.
add_test(NAME app.ui-windows-thread-fit
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-thread-fit-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-thread-fit"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-thread-fit PROPERTIES TIMEOUT 240)
