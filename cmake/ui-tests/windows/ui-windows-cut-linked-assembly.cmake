# SPDX-License-Identifier: MIT
# Extrude Cut in an assembly on a linked part (mitcad#2131): Affected
# components lists the linked occurrence, checked it is machined in this
# assembly only, OK cuts it and the part's design file stays as it was. The
# Linux test's shared input script (tools/ui-cut-linked-assembly-test.sh).
add_test(NAME app.ui-windows-cut-linked-assembly
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-cut-linked-assembly-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-cut-linked-assembly"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-cut-linked-assembly PROPERTIES TIMEOUT 300)
