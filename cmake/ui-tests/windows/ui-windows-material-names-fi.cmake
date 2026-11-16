# SPDX-License-Identifier: MIT
# The built-in materials' names in Finnish (mitcad#1689): the Material
# tool's default and picker and the Materials window in Finnish, the
# saved design storing no Finnish name. The Linux test's shared input
# script (tools/ui-material-names-fi-test.sh).
add_test(NAME app.ui-windows-material-names-fi
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-material-names-fi-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-material-names-fi"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-material-names-fi PROPERTIES TIMEOUT 300)
