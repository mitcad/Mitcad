# SPDX-License-Identifier: MIT
# Preferences > Display > Theme (mitcad#1330, GitHub mitcad/Mitcad#151):
# started with Theme Dark on the light app mode, the Start page, the
# Browser's card, the Browser and the timeline dark; Light and Dark from
# Preferences at once; System follows the app mode again; Windows 11's
# style (Floating, Docked) and Windows 10's path forced. The Linux test
# is tools/ui-theme-preference-test.sh.
add_test(NAME app.ui-windows-theme-preference
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-theme-preference-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-theme-preference"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-theme-preference PROPERTIES TIMEOUT 600)
