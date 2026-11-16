# SPDX-License-Identifier: MIT
# My library without a design (mitcad#704, mitcad#1135): the managers
# opened with no design open; a built-in material duplicated into My
# library, renamed, its density changed and deleted, a printer
# duplicated, renamed and deleted, each in the library's files; the
# window's design unchanged.
add_test(NAME app.ui-windows-my-library
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-my-library-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-my-library" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-my-library PROPERTIES TIMEOUT 300)
