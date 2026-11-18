# SPDX-License-Identifier: MIT
# Measure's main distance and axis distance drawn apart where they overlap,
# each row painting its own segment (mitcad#1979); the same shared script
# as Linux and macOS.
add_test(NAME app.ui-windows-measure-guides
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-measure-guides-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-measure-guides"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-measure-guides PROPERTIES TIMEOUT 180)
