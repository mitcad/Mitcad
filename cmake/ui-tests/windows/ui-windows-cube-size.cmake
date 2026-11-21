# SPDX-License-Identifier: MIT
# The orientation cube's size (mitcad#2071, GitHub mitcad/Mitcad#447): 75 %
# and 150 % from the cube's menu and from Preferences, the whole corner
# scaled, no buttons overlapping, the Command card below the larger cube.
# The Linux test's shared input script (tools/ui-cube-size-test.sh).
add_test(NAME app.ui-windows-cube-size
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cube-size-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-cube-size"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-cube-size PROPERTIES TIMEOUT 300)
