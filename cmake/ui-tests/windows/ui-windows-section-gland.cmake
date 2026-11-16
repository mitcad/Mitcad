# SPDX-License-Identifier: MIT
# A cable gland body with modelled threads cut by Section Analysis in
# its profile's plane and 1 mm off it, with caps (mitcad#1230); the
# Linux test's shared input script.
add_test(NAME app.ui-windows-section-gland
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-section-gland-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-section-gland"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-section-gland PROPERTIES TIMEOUT 360)
