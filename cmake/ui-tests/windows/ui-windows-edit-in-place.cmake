# SPDX-License-Identifier: MIT
# Edit in Place (mitcad#2321): a linked part edited inside the assembly
# that holds it, its document taken from its open tab, a sketch on another
# component's face frozen with its notice, Save writing the part's file,
# Finish returning to the holder with one undo step. The Linux test is
# tools/ui-edit-in-place-test.sh.
add_test(NAME app.ui-windows-edit-in-place
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-edit-in-place-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-edit-in-place" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-edit-in-place PROPERTIES TIMEOUT 420)
