# SPDX-License-Identifier: MIT
# Offset Plane's Reverse Normal and signed Distance (mitcad#2048): the
# inputs' names and tooltips, the preview's normal marker turned over, a
# negative distance on the other side of the face. The Linux test's shared
# input script (tools/ui-construct-reverse-normal-test.sh).
add_test(NAME app.ui-windows-construct-reverse-normal
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-construct-reverse-normal-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-construct-reverse-normal"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-construct-reverse-normal PROPERTIES TIMEOUT 300)
