# SPDX-License-Identifier: MIT
# With a renderer the app can run in ctest: screenshots of a demo block
# and of a project file opened from the command line.
add_test(NAME app.ui-windows
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
