# SPDX-License-Identifier: MIT
# Move/Copy's Free Move handles (mitcad#1580, #1581, #1583): an edit's
# rings where the feature moves what it moves (a body, a component, a
# linked component, a turned body), OK without changes keeps the
# matrix, the arrows meet at the moved middle, the axis colors; the
# Linux test's shared input script.
add_test(NAME app.ui-windows-move-handles
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-move-handles-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-move-handles"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-move-handles PROPERTIES TIMEOUT 360)
