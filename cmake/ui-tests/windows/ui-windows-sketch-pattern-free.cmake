# SPDX-License-Identifier: MIT
# Sketch patterns (mitcad#2007): a deleted copy stays deleted, Trim on a
# copy frees the pattern with a note, Free Pattern in the context menu frees
# another. The Linux test's shared input script
# (tools/ui-sketch-pattern-free-test.sh).
add_test(NAME app.ui-windows-sketch-pattern-free
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-pattern-free-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-pattern-free"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-pattern-free PROPERTIES TIMEOUT 300)
