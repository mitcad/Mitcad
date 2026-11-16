# SPDX-License-Identifier: MIT
# Property Sets (mitcad#496): an occurrence's and its component's
# levels, a value edited in place, an own type's value as a choice,
# Find in Design by property and the Parts List's property columns.
add_test(NAME app.ui-windows-property-sets
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-property-sets-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-property-sets"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-property-sets PROPERTIES TIMEOUT 300)
