# SPDX-License-Identifier: MIT
# The outline set (mitcad#2194): Show profiles leaves a plate's holes
# unfilled but picked, Extrude starts with the plate and its island and
# takes the same for a whole sketch picked on its timeline cell or Browser
# row, and a sketch whose closed curves cross is said in the panel; the
# Linux test's shared input script.
add_test(NAME app.ui-windows-outline-set
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-outline-set-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-outline-set"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-outline-set PROPERTIES TIMEOUT 240)
