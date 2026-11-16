# SPDX-License-Identifier: MIT
# Placement by picked geometry (mitcad#656): earlier placements opened
# through Place On, a box on a face from its edges, turned in the view.
add_test(NAME app.ui-windows-place
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-place-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-place"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-place PROPERTIES TIMEOUT 300)
