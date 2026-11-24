# SPDX-License-Identifier: MIT
# Two bearings on one shaft joined by a Rigid joint or a Rigid Group move
# together (mitcad#2274): on the reported design of the private test files
# (MITCAD_MITCAD_REPORTED); skipped without it. The Linux test's shared
# input script (tools/ui-carriages-together-test.sh).
add_test(NAME app.ui-windows-carriages-together
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-carriages-together-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-carriages-together"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-carriages-together PROPERTIES
  TIMEOUT 400 SKIP_RETURN_CODE 77)
