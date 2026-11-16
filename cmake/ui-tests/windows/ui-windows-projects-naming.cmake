# SPDX-License-Identifier: MIT
# Names typed in the Projects panel (mitcad#1268): the proposed name free
# on disk, also while a remote fetch runs; a refused name keeps the
# editor with its message; the collapsed card expanded and the hidden
# dock shown for the name, then put back; the Linux test's shared input
# scripts.
add_test(NAME app.ui-windows-projects-naming
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-projects-naming-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-projects-naming"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-projects-naming PROPERTIES TIMEOUT 420)
