# SPDX-License-Identifier: MIT
# The Make card's Send to Slicer (mitcad#1377), the Linux test's shared
# input script: with a face of one of two shown bodies selected, the
# card's button opens Send to Slicer's panel with that body alone, and
# Send writes its file only.
add_test(NAME app.ui-windows-make-card-send
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-make-card-send-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-make-card-send"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-make-card-send PROPERTIES TIMEOUT 300)
