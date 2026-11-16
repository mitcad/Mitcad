# SPDX-License-Identifier: MIT
# Configurations (mitcad#807): the window opens and a choice in the
# Configuration bar changes the body (the test driver's
# script, shared with the macOS test).
add_test(NAME app.ui-windows-configurations
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-configurations-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-configurations" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-configurations PROPERTIES TIMEOUT 360)
