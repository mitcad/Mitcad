# SPDX-License-Identifier: MIT
# The 3D Sketch Palette's rows in words (mitcad#1652): no ids,
# topological names or coordinates; the pointer over the reference
# curve's row paints it and the edge it follows. The Linux test's shared
# input script (tools/ui-sketch3d-palette-words-test.sh).
add_test(NAME app.ui-windows-sketch3d-palette-words
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch3d-palette-words-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch3d-palette-words"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch3d-palette-words PROPERTIES TIMEOUT 300)
