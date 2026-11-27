# SPDX-License-Identifier: MIT
# Export Turntable (mitcad#2523): the Linux test's shared input script
# (tools/ui-turntable-test.sh, tools/ui-scripts/turntable.mitcad-ui): 36
# frames of a full turn stepping 10 degrees, and with FFmpeg on the PATH
# the MP4 video's frame count and size.
add_test(NAME app.ui-windows-turntable
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-turntable-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-turntable"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-turntable PROPERTIES TIMEOUT 360)
