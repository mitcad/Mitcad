# SPDX-License-Identifier: MIT
# Anaglyph 3D (mitcad#1449): View > Anaglyph 3D's stereo camera, a pick
# in it, Preferences > Display's stereo depth and convergence, and the
# projection back when it ends. The Linux test's shared input script
# (tools/ui-view-anaglyph-test.sh).
add_test(NAME app.ui-windows-view-anaglyph
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-anaglyph-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-view-anaglyph"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-view-anaglyph PROPERTIES TIMEOUT 180)
