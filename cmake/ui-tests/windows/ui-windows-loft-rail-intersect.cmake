# SPDX-License-Identifier: MIT
# A loft from a circle to a square through a rail tied to Intersect points,
# and a loose rail's miss in words (mitcad#1944). The Linux test's shared
# input script (tools/ui-loft-rail-intersect-test.sh).
add_test(NAME app.ui-windows-loft-rail-intersect
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-loft-rail-intersect-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-loft-rail-intersect"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-loft-rail-intersect PROPERTIES TIMEOUT 300)
