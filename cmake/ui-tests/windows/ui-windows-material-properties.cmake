# SPDX-License-Identifier: MIT
# Materials' mechanical and thermal properties (mitcad#1213), as the Linux
# test: a value typed in psi in an inch design, a material of the
# bundled dataset created and saved, a CSV imported with its invalid
# row reported.
add_test(NAME app.ui-windows-material-properties
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-material-properties-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-material-properties"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-material-properties PROPERTIES TIMEOUT 300)
