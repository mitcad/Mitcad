# SPDX-License-Identifier: MIT
# The rendered view from below the ground (mitcad#320): orbited under
# the ground and back, orthographic and perspective, the box rendered
# in every view, the ground left out below and built again above.
add_test(NAME app.ui-windows-render-ground
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-render-ground-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-render-ground"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-render-ground PROPERTIES TIMEOUT 600)
