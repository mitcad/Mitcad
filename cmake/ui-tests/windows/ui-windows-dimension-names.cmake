# SPDX-License-Identifier: MIT
# Dimension names (mitcad#231): the tooltip, the name in the value
# editor, a dimension picked into it, a rename, Dimension text, and a
# feature panel's field with its parameter's name.
add_test(NAME app.ui-windows-dimension-names
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-dimension-names-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-dimension-names"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-dimension-names PROPERTIES TIMEOUT 300)
