# SPDX-License-Identifier: MIT
# Surface Loft's Start and End Condition (mitcad#2345): Perpendicular to
# Face at both ends, the surface one side face. The Linux test's shared
# input script (tools/ui-surface-loft-perpendicular-test.sh).
add_test(NAME app.ui-windows-surface-loft-perpendicular
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-surface-loft-perpendicular-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-surface-loft-perpendicular"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-surface-loft-perpendicular PROPERTIES TIMEOUT 300)
