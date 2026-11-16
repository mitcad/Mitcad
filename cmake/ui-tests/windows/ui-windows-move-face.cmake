# SPDX-License-Identifier: MIT
# MODIFY > Move Face (mitcad#531): a block's end face pushed out, its
# timeline cell's card and edit, its top tilted about an edge, a motion
# the neighbouring faces cannot follow refused on the panel.
add_test(NAME app.ui-windows-move-face
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-move-face-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-move-face"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-move-face PROPERTIES TIMEOUT 300)
