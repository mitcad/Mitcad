# SPDX-License-Identifier: MIT
# Constraints the drawn lines already satisfy, added when safe
# (mitcad#1024): a quadrant's coincident and centre alignment, a
# horizontal between level points, a tangent where a line touches a
# circle, and a rectangle afterwards with nothing refused.
add_test(NAME app.ui-windows-sketch-auto-constraints
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-auto-constraints-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-auto-constraints" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-auto-constraints PROPERTIES TIMEOUT 300)
