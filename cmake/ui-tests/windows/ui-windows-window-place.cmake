# SPDX-License-Identifier: MIT
# The design window's size, place and state kept between starts
# (mitcad#1710, GitHub mitcad/Mitcad#283): resized and moved, the same
# at the next start, maximized kept, a new window at the last size, and
# a kept place off the screens moved onto the primary one. The Linux
# test's shared input scripts (tools/ui-window-place-test.sh).
add_test(NAME app.ui-windows-window-place
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-window-place-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-window-place"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-window-place PROPERTIES TIMEOUT 420)
