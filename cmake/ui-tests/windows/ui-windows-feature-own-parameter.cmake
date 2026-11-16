# SPDX-License-Identifier: MIT
# A command panel value that uses the field's own parameter keeps that
# parameter as a user parameter (mitcad#1536): depth * 2 in Edit
# Extrude's Distance of depth. The Linux test's shared input script
# (tools/ui-feature-own-parameter-test.sh).
add_test(NAME app.ui-windows-feature-own-parameter
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-feature-own-parameter-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-feature-own-parameter"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-feature-own-parameter PROPERTIES TIMEOUT 180)
