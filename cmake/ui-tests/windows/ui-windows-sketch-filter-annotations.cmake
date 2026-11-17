# SPDX-License-Identifier: MIT
# The sketch Selection Filter's Dimensions and Constraints rows (GitHub
# #388): a window with only Dimensions a cross takes only the dimension,
# with only Constraints a cross only the constraints; a click follows its
# own row. The Linux test's shared input script
# (tools/ui-sketch-filter-annotations-test.sh).
add_test(NAME app.ui-windows-sketch-filter-annotations
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-filter-annotations-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-filter-annotations"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-filter-annotations PROPERTIES TIMEOUT 180)
