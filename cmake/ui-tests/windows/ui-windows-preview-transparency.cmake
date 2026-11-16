# SPDX-License-Identifier: MIT
# A new sphere's see-through preview without a pattern of triangles
# (mitcad#1575, GitHub mitcad/Mitcad#219) on Mesa's software renderer:
# the shared steps of tools/ui-preview-transparency-test.sh.
add_test(NAME app.ui-windows-preview-transparency
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-preview-transparency-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-preview-transparency"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-preview-transparency PROPERTIES TIMEOUT 180)
