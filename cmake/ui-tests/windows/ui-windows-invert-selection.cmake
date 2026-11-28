# SPDX-License-Identifier: MIT
# Invert Selection (mitcad#2647): F7 bound in Keyboard Shortcuts; bodies
# inverted (the hidden one not taken), the faces of an isolated body, a
# sketch's lines by the key and the view's context menu, and Interference's
# Bodies input. The Linux test's shared input script
# (tools/ui-invert-selection-test.sh).
add_test(NAME app.ui-windows-invert-selection
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-invert-selection-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-invert-selection"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-invert-selection PROPERTIES TIMEOUT 200)
