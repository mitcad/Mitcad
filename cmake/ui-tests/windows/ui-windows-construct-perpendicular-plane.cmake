# SPDX-License-Identifier: MIT
# Plane Perpendicular to Face at Point (mitcad#2111): offered by the view's
# context menu for a face and a point, made level through the face's
# nearest point, turned by its Angle, and a sketch on it takes its frame.
# The Linux test's shared input script
# (tools/ui-construct-perpendicular-plane-test.sh).
add_test(NAME app.ui-windows-construct-perpendicular-plane
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-construct-perpendicular-plane-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-construct-perpendicular-plane"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-construct-perpendicular-plane PROPERTIES TIMEOUT 300)
