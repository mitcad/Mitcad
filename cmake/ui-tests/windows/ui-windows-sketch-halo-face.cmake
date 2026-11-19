# SPDX-License-Identifier: MIT
# Fully constrained curves' halos on a light background only over a dark
# face, following the appearances (mitcad#2034), shared with Linux and macOS.
add_test(NAME app.ui-windows-sketch-halo-face
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-halo-face-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-halo-face"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-halo-face PROPERTIES TIMEOUT 300)
