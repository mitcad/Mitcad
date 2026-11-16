# SPDX-License-Identifier: MIT
# A press on the view stays what it began as until its button is up
# (mitcad#1571, #1572, #1573): keys changed during it, a trackpad scroll
# while a button is held, the window deactivated during a drag and F4
# released elsewhere, in the Floating and the Docked layout. The Linux
# test's shared input script (tools/ui-navigation-gesture-state-test.sh).
add_test(NAME app.ui-windows-navigation-gesture-state
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-navigation-gesture-state-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-navigation-gesture-state"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-navigation-gesture-state PROPERTIES TIMEOUT 420)
