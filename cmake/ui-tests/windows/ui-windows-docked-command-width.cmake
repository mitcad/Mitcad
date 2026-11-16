# SPDX-License-Identifier: MIT
# The Docked Command panel at Text size and Icon size 200 % (mitcad#1825,
# GitHub mitcad/Mitcad#336): at least 600 px wide, the Sketch Palette's
# rows whole within it, its edge dragged wider remembered and restored
# after a restart. The Linux test's shared input scripts
# (tools/ui-docked-command-width-test.sh).
add_test(NAME app.ui-windows-docked-command-width
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-docked-command-width-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-docked-command-width"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-docked-command-width PROPERTIES TIMEOUT 300)
