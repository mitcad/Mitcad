# SPDX-License-Identifier: MIT
# The 3D view's Follow System background follows MitCad's theme
# (mitcad#2145): with no saved background the dark gradient under the dark
# theme (by preference and by the app mode), the light one under the light
# theme, at once when the theme changes, and again after Restore Defaults.
# The Linux test's shared input script
# (tools/ui-view-background-theme-test.sh).
add_test(NAME app.ui-windows-view-background-theme
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-background-theme-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-view-background-theme"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-view-background-theme PROPERTIES TIMEOUT 300)
