# SPDX-License-Identifier: MIT
# How a new design opens (mitcad#1683): looking at Top with its origin
# shown and centred, with no undo step; designs read from files keep
# their saved origin and open in the isometric view; New Design after
# one looks at Top again (tools/ui-new-design-start-test.sh).
add_test(NAME app.ui-windows-new-design-start
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-new-design-start-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-new-design-start"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-new-design-start PROPERTIES TIMEOUT 300)
