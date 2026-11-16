# SPDX-License-Identifier: MIT
# A flexible subassembly: Hinge:1 made Flexible from its row's menu and
# its inner Arm dragged along the Hinge's own joint, for Hinge:1 alone
# (mitcad#1793). The Linux test's shared input script
# (tools/ui-flexible-subassembly-test.sh).
add_test(NAME app.ui-windows-flexible-subassembly
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-flexible-subassembly-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-flexible-subassembly"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-flexible-subassembly PROPERTIES TIMEOUT 300)
