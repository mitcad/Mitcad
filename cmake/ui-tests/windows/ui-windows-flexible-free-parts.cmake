# SPDX-License-Identifier: MIT
# Flexible on a subassembly without joints of its own: Gantry:1 made
# Flexible from its row's menu and its free Joiner dragged along a joint
# of the root, alone (mitcad#2016). The Linux test's shared input script
# (tools/ui-flexible-free-parts-test.sh).
add_test(NAME app.ui-windows-flexible-free-parts
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-flexible-free-parts-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-flexible-free-parts"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-flexible-free-parts PROPERTIES TIMEOUT 300)
