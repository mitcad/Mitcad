# SPDX-License-Identifier: MIT
# A sketch region inside a body picked before the body's face in front
# of it while an input takes profiles (mitcad#1222); the Linux test's
# shared input script.
add_test(NAME app.ui-windows-region-pick-inside-body
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-region-pick-inside-body-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-region-pick-inside-body"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-region-pick-inside-body PROPERTIES TIMEOUT 240)
