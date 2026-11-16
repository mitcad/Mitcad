# SPDX-License-Identifier: MIT
# The orbit's center (mitcad#1791): Set Orbit Center Here from the
# view's context menu, an orbit that keeps that point's place, Clear
# Orbit Center, and an orbit about the point under the cursor; the
# Linux test's shared input script (tools/ui-view-orbit-center-test.sh).
add_test(NAME app.ui-windows-view-orbit-center
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-orbit-center-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-view-orbit-center"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-view-orbit-center PROPERTIES TIMEOUT 240)
