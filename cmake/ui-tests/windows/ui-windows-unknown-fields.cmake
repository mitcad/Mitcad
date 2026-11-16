# SPDX-License-Identifier: MIT
# A design a newer MitCad wrote (mitcad#1132): the warning on opening
# it in both layouts, and the question before Save writes over it
# (Cancel, Save As, Save and Drop Them).
add_test(NAME app.ui-windows-unknown-fields
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-unknown-fields-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-unknown-fields"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-unknown-fields PROPERTIES TIMEOUT 300)
