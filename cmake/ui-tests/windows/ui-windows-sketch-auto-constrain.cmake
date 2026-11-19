# SPDX-License-Identifier: MIT
# Auto Constrain (mitcad#2019): a rectangle of four lines without
# constraints ends fully constrained, the relations only leave 4 degrees of
# freedom, and OK is one undo step. The Linux test's shared input script
# (tools/ui-sketch-auto-constrain-test.sh).
add_test(NAME app.ui-windows-sketch-auto-constrain
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-auto-constrain-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-auto-constrain"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-auto-constrain PROPERTIES TIMEOUT 180)
