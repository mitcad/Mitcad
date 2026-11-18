# SPDX-License-Identifier: MIT
# A large design's drawing (mitcad#1997): drawn in a job with its note, a
# new scale's old lines scaled meanwhile, a view's placing cancelled and
# projected once. The Linux test's shared input script
# (tools/ui-drawing-large-view-test.sh).
add_test(NAME app.ui-windows-drawing-large-view
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-drawing-large-view-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-drawing-large-view"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-drawing-large-view PROPERTIES TIMEOUT 300)
