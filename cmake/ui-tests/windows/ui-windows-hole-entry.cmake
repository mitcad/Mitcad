# SPDX-License-Identifier: MIT
# Hole's keyboard and its dimensions in the view (mitcad#1181), as
# tools/ui-hole-entry-test.sh: Tab from Distance 2 to Diameter and back,
# Distance 2 and Depth typed in their value boxes (Enter, Esc, a text
# that is not a value refused), and a reference picked in its row
# (mitcad#1285: the distances evaluated, the hole where it was).
add_test(NAME app.ui-windows-hole-entry
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-hole-entry-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-hole-entry"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-hole-entry PROPERTIES TIMEOUT 450)
