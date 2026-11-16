# SPDX-License-Identifier: MIT
# Tools › Libraries (mitcad#1250): the menu in its groups; Material and
# Printer Library… on My library with no design open, an item edited
# in its file; Process Library…; the Material tool's Material
# Library…, whose new material the tool offers, its bodies kept.
add_test(NAME app.ui-windows-libraries-menu
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-libraries-menu-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-libraries-menu" -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-libraries-menu PROPERTIES TIMEOUT 300)
