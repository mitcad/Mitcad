# SPDX-License-Identifier: MIT
# A Sketch Fillet refused for conflicting constraints (mitcad#1566): the
# panel says in words, without ids, that the Equal can't hold with the
# fillet, and the Equal's symbol and lines are painted in the view, in
# the hover's color over the message. The Linux test's shared input
# script (tools/ui-sketch-conflict-test.sh).
add_test(NAME app.ui-windows-sketch-conflict
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch-conflict-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch-conflict"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch-conflict PROPERTIES TIMEOUT 300)
