# SPDX-License-Identifier: MIT
# Decals (mitcad#2244): the Linux test's shared input script
# (tools/ui-decal-test.sh, tools/ui-scripts/decal.mitcad-ui).
add_test(NAME app.ui-windows-decal
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-decal-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-decal"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-decal PROPERTIES TIMEOUT 240)
