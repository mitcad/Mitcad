# SPDX-License-Identifier: MIT
# Joint conditions of profiles along sketch lines (mitcad#2200): Profile
# along sketch lines with Corners Butt, an L swapped in Ends, a T set to Far
# Side with Shift + click. The Linux test's shared input script
# (tools/ui-build-joints-test.sh).
add_test(NAME app.ui-windows-build-joints
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-joints-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-build-joints"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-build-joints PROPERTIES TIMEOUT 360)
