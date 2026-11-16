# SPDX-License-Identifier: MIT
# Clean Up Projections (mitcad#232): a face projected twice into a
# sketch on it, the repeats listed by the preview and removed; a
# coincident that replaces a Horizontal and a refused snap (mitcad#287).
add_test(NAME app.ui-windows-projections
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-projections-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-projections"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-projections PROPERTIES TIMEOUT 420)
