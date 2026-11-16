# SPDX-License-Identifier: MIT
# Mortise and Tenon (mitcad#1012): a tenon on a rail's end face and its
# mortise in a post, both bodies changed, then edited.
add_test(NAME app.ui-windows-mortise-tenon
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-mortise-tenon-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-mortise-tenon"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-mortise-tenon PROPERTIES TIMEOUT 300)
