# SPDX-License-Identifier: MIT
# Reverse vertical pan (mitcad#1873, GitHub mitcad/Mitcad#340): off by
# default, a two-finger scroll reversed with it on, saved and read back
# by a new process. The Linux test's shared input scripts
# (tools/ui-navigation-reverse-pan-test.sh).
add_test(NAME app.ui-windows-navigation-reverse-pan
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-navigation-reverse-pan-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-navigation-reverse-pan"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-navigation-reverse-pan PROPERTIES TIMEOUT 420)
