# SPDX-License-Identifier: MIT
# The Floating Command card at Text size and Icon size 200 % (mitcad#1846,
# follow-up of mitcad#1825): at least 600 px wide, the Sketch Palette's
# rows whole within it and no button clipped. The Linux test's shared
# input script (tools/ui-floating-command-width-test.sh).
add_test(NAME app.ui-windows-floating-command-width
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-floating-command-width-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-floating-command-width"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-floating-command-width PROPERTIES TIMEOUT 300)
