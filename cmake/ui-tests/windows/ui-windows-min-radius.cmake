# SPDX-License-Identifier: MIT
# Minimum Radius (mitcad#2518): the smallest radius of a rounded block marked,
# the roundings under a limit of 11 mm and of 4 mm; kept as MinRadius1,
# hidden, shown, edited, saved, deleted and undone. The Linux test's shared
# input script (tools/ui-min-radius-test.sh).
add_test(NAME app.ui-windows-min-radius
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-min-radius-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-min-radius"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-min-radius PROPERTIES TIMEOUT 360)
