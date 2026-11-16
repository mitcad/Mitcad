# SPDX-License-Identifier: MIT
# A body's Browser row and View › Show (mitcad#1287, GitHub
# mitcad/Mitcad#142): Delete on a row of an imported base feature's
# body removes only that body (Docked); View › Show › Construction,
# Sketches and Origins hide and show them in two components without
# changing a light bulb (Floating). The Linux test is
# tools/ui-view-show-test.sh.
add_test(NAME app.ui-windows-view-show
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-view-show-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-view-show" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-view-show PROPERTIES TIMEOUT 300)
