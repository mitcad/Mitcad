# SPDX-License-Identifier: MIT
# Favorite Parameters (mitcad#2203, GitHub mitcad/Mitcad#547): the card
# from MANAGE in the Floating and Docked layouts: two favorites starred, a
# typed value, the slider, a refused value, undo, unstarred, the empty
# state, the toggle and its own close button. The Linux test's shared input
# script (tools/ui-favorite-parameters-test.sh).
add_test(NAME app.ui-windows-favorite-parameters
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-favorite-parameters-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-favorite-parameters"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-favorite-parameters PROPERTIES TIMEOUT 900)
