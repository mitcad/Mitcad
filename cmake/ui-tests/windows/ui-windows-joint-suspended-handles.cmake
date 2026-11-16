# SPDX-License-Identifier: MIT
# A command panel set aside hides its handles (mitcad#1844): Joint's
# Animate, a drag where a hidden handle was moving nothing, the handles
# back after Esc. The Linux test's shared input script
# (tools/ui-joint-suspended-handles-test.sh).
add_test(NAME app.ui-windows-joint-suspended-handles
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-joint-suspended-handles-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-joint-suspended-handles"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-joint-suspended-handles PROPERTIES TIMEOUT 240)
