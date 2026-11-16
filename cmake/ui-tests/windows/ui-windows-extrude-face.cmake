# SPDX-License-Identifier: MIT
# Extrude with a planar body face as the profile (mitcad#535): a face
# of an imported cube picked in the command and joined, a sketched
# cube's side selected before it (not the unused square beside it) cut
# in.
add_test(NAME app.ui-windows-extrude-face
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-extrude-face-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-extrude-face"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-extrude-face PROPERTIES TIMEOUT 300)
