# SPDX-License-Identifier: MIT
# The window's layout chosen in Preferences (mitcad#419): Floating
# asks to restart, Later and Restart Now (unsaved changes asked about,
# the restart's command logged), the next start Floating, --chrome
# overriding it; the Floating cards below the top bar at the default
# size and in a narrow and a wide window.
add_test(NAME app.ui-windows-chrome-layout
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-chrome-layout-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-chrome-layout"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-chrome-layout PROPERTIES TIMEOUT 300)
