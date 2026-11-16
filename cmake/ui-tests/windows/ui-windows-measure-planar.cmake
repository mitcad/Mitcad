# SPDX-License-Identifier: MIT
# Same synthetic planar Measure workflow as Linux/macOS (mitcad#1161).
add_test(NAME app.ui-windows-measure-planar
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-measure-planar-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-measure-planar"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-measure-planar PROPERTIES TIMEOUT 180)
