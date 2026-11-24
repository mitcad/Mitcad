# SPDX-License-Identifier: MIT
# URDF export for ROS 2 (mitcad#2100): File > Export to .urdf writes the
# package folder. The Linux test's shared input script (tools/ui-urdf-test.sh).
add_test(NAME app.ui-windows-urdf
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-urdf-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-urdf"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-urdf PROPERTIES TIMEOUT 300)
