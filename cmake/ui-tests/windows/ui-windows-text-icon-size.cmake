# SPDX-License-Identifier: MIT
# Preferences' Text size, Icon size and Font (mitcad#1556, GitHub
# mitcad/Mitcad#207): four starts at other sizes and in another font,
# the font, the icons and the layout checked. The Linux test's shared
# input script (tools/ui-text-icon-size-test.sh).
add_test(NAME app.ui-windows-text-icon-size
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-text-icon-size-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-text-icon-size"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-text-icon-size PROPERTIES TIMEOUT 600)
