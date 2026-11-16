# SPDX-License-Identifier: MIT
# The Docked layout's window height (mitcad#560): a design of 30
# bodies, Root/Bodies opened and the pattern's 29 bodies selected from
# the timeline; the Browser's rows scroll, the command panel's
# selection lines scroll, and the window, the view and the timeline's
# cells keep their sizes and places; a plain double-click edits.
add_test(NAME app.ui-windows-docked-height
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-docked-height-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-docked-height"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-docked-height PROPERTIES TIMEOUT 300)
