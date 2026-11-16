# SPDX-License-Identifier: MIT
# The Configuration panel and the left column (mitcad#1020): the
# Configuration card above the browser's, the browser's card down to the
# timeline, the boundary dragged and remembered after a restart, a
# double-click for the automatic split, the card collapsed.
add_test(NAME app.ui-windows-configuration-panel
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-configuration-panel-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-configuration-panel"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-configuration-panel PROPERTIES TIMEOUT 300)
