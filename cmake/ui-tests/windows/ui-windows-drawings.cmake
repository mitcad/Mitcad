# SPDX-License-Identifier: MIT
# Drawings kept in the design (mitcad#707): Create Drawing, a base, a
# projected, a section and a detail view, the model changed under them,
# saved and opened again, a PDF exported.
add_test(NAME app.ui-windows-drawings
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawings-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawings"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawings PROPERTIES TIMEOUT 300)
