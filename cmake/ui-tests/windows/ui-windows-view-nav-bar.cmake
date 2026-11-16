# SPDX-License-Identifier: MIT
# The navigation bar at the bottom of the view (mitcad#1086), in the
# Floating and the Docked layout: shown; Snap to Grid and Layout Grid
# in step with the View menu's; Fit; Look At in a sketch looks at its
# plane; the palette's Snap to grid; Preferences hides and shows it.
add_test(NAME app.ui-windows-view-nav-bar
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-nav-bar-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-view-nav-bar"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-view-nav-bar PROPERTIES TIMEOUT 300)
