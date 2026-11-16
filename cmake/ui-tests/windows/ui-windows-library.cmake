# SPDX-License-Identifier: MIT
# The library browser's Update (mitcad#317): empty until Update, which
# fetches every enabled source (a local git repository) and lists its
# parts; a broken source reported in one warning, the selection kept.
add_test(NAME app.ui-windows-library
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-library-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-library"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-library PROPERTIES TIMEOUT 300)
