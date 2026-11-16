# SPDX-License-Identifier: MIT
# Tangent Plane (mitcad#446): the reference plane and angle filled in
# from the click, the preview drawn over the picked plane.
add_test(NAME app.ui-windows-tangent-plane
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-tangent-plane-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-tangent-plane"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-tangent-plane PROPERTIES TIMEOUT 300)
