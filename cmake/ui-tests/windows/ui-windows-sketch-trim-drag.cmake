# SPDX-License-Identifier: MIT
# Trim's drag (mitcad#2524): a sweep across three pieces trimmed as one
# undo step, Esc during a drag, a plain click and a window selection after
# Trim. The Linux test's shared input script
# (tools/ui-sketch-trim-drag-test.sh).
add_test(NAME app.ui-windows-sketch-trim-drag
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-trim-drag-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-trim-drag"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-trim-drag PROPERTIES TIMEOUT 300)
