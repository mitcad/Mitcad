# SPDX-License-Identifier: MIT
# A My library material whose file lacks its own color (mitcad#1242):
# the Materials window renames it, the new name listed and its own
# color made again in the file (the Linux test's shared driver script).
add_test(NAME app.ui-windows-library-material-own-colour
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-library-material-own-colour-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-library-material-own-colour"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-library-material-own-colour PROPERTIES TIMEOUT 300)
