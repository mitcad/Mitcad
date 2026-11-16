# SPDX-License-Identifier: MIT
# Bearing (mitcad#577): a 608 from the panel, too many elements, edited,
# then a raceway cut into a shaft made by mitcad-cli.
add_test(NAME app.ui-windows-bearing
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-bearing-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-bearing"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-bearing PROPERTIES TIMEOUT 300)
