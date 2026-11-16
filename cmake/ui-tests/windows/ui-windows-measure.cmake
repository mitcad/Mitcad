# SPDX-License-Identifier: MIT
# Measure (mitcad#647): a round face's axis shown and drawn, parallel
# axes 50 apart, a round face square to a plane, two rims' axes and
# centres.
add_test(NAME app.ui-windows-measure
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-measure-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-measure"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
# Its Copy and Copy All write the system clipboard, which every test of
# one ssh logon shares: not at once with the other tests that use it
# (app.ui-windows-sketch-clipboard, app.ui-windows-assistant-setup).
set_tests_properties(app.ui-windows-measure PROPERTIES TIMEOUT 300 RESOURCE_LOCK system_clipboard)
