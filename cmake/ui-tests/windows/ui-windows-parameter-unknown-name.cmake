# SPDX-License-Identifier: MIT
# An unknown name and Enter make a user parameter in a drawing tool's
# field, a sketch dimension and a command panel's field, and name =
# value typed out still names the field's own parameter (mitcad#1723).
# The Linux test's shared input script
# (tools/ui-parameter-unknown-name-test.sh).
add_test(NAME app.ui-windows-parameter-unknown-name
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-parameter-unknown-name-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-parameter-unknown-name"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-parameter-unknown-name PROPERTIES TIMEOUT 300)
