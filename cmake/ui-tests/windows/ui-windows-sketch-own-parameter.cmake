# SPDX-License-Identifier: MIT
# A sketch dimension's value that uses its own parameter keeps that
# parameter as a user parameter (mitcad#1535): BuckleDiameter * 2 in the
# dimension of BuckleDiameter, and Move to User Parameters from the
# context menu. The Linux test's shared input script
# (tools/ui-sketch-own-parameter-test.sh).
add_test(NAME app.ui-windows-sketch-own-parameter
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-own-parameter-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-own-parameter"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-own-parameter PROPERTIES TIMEOUT 180)
