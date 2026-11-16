# SPDX-License-Identifier: MIT
# The 3D sketch manipulator's keys (Tab, Shift+Tab, Up Arrow) and Copy
# and Paste between 3D sketches (mitcad#1661). The Linux test's shared
# input script (tools/ui-sketch3d-keys-clipboard-test.sh).
add_test(NAME app.ui-windows-sketch3d-keys-clipboard
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-sketch3d-keys-clipboard-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-sketch3d-keys-clipboard"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-sketch3d-keys-clipboard PROPERTIES TIMEOUT 300 RESOURCE_LOCK system_clipboard)
