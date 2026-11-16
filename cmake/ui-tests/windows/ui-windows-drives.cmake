# SPDX-License-Identifier: MIT
# Gear Pair and Belt Drive (mitcad#656): both parts of a ratio between
# two shafts made by mitcad-cli, a row of the candidates chosen.
add_test(NAME app.ui-windows-drives
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drives-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drives"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drives PROPERTIES TIMEOUT 300)
