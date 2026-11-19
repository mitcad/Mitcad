# SPDX-License-Identifier: MIT
# Flat Pack (mitcad#1939) (mitcad#1939): the Linux test's shared input script
# (tools/ui-flat-pack-test.sh, tools/ui-scripts/flat-pack.mitcad-ui).
add_test(NAME app.ui-windows-flat-pack
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-flat-pack-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-flat-pack"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-flat-pack PROPERTIES TIMEOUT 240)
