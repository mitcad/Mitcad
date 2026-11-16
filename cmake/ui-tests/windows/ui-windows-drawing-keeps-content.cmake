# SPDX-License-Identifier: MIT
# A drawing keeps what was put into it (mitcad#1293): a component hidden
# in the browser and one pasted later change no view; its Shows adds
# one, Update from Model takes what the 3D view shows, each one undo
# step; an older file's view is pinned when it opens.
add_test(NAME app.ui-windows-drawing-keeps-content
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-keeps-content-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-keeps-content"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-keeps-content PROPERTIES TIMEOUT 300)
