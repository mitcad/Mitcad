# SPDX-License-Identifier: MIT
# File > Save to Project (mitcad#503): after Save As, no Move to a
# Project in the menu; the old command id's saved shortcut still starts
# it; a loose design saved in a project under a typed name (the
# original stays); Save with Description's question opens the picker.
add_test(NAME app.ui-windows-save-to-project
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-save-to-project-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-save-to-project"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-save-to-project PROPERTIES TIMEOUT 300)
