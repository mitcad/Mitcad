# SPDX-License-Identifier: MIT
# Pulley (mitcad#562): a GT2 pulley from the panel, its teeth and
# flanges changed, edited, a second pulley on the belt (its length).
add_test(NAME app.ui-windows-pulley
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-pulley-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-pulley"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-pulley PROPERTIES TIMEOUT 300)
