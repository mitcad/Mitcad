# SPDX-License-Identifier: MIT
# Project Again's choice of the edge that takes a lost projection's
# source's place: the rows in words, the hovered row's edge painted, the
# clicked one kept (mitcad#1794). The Linux test's shared input script
# (tools/ui-project-again-successor-test.sh).
add_test(NAME app.ui-windows-project-again-successor
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-project-again-successor-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-project-again-successor"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-project-again-successor PROPERTIES TIMEOUT 300)
