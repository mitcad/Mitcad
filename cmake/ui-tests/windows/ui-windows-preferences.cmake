# SPDX-License-Identifier: MIT
# Preferences' size (mitcad#690): with git found, MITCAD_GIT naming a
# missing file and git not installed, the window fits a 1366 x 768
# screen, is resizable, its frame lies on the screen and the page list
# shows its entries whole.
add_test(NAME app.ui-windows-preferences
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-preferences-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-preferences"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-preferences PROPERTIES TIMEOUT 300)
