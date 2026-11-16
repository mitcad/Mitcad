# SPDX-License-Identifier: MIT
# A drag zoom follows the pointer's vertical motion (mitcad#1867, GitHub
# mitcad/Mitcad#330): Shift + middle in the middle-orbit scheme, up
# closer, down away, sideways alone not at all, in the Floating and the
# Docked layout. The Linux test's shared input script
# (tools/ui-navigation-drag-zoom-test.sh).
add_test(NAME app.ui-windows-navigation-drag-zoom
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-navigation-drag-zoom-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-navigation-drag-zoom"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-navigation-drag-zoom PROPERTIES TIMEOUT 300)
