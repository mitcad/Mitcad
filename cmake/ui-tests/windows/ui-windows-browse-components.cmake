# SPDX-License-Identifier: MIT
# Browse Components (mitcad#1283, GitHub #136): Tools › Libraries lists
# it once; Library Sources… from the browser; a component of a library
# an index names shows Get Library, stays selected after the fetch and
# is inserted; Installed, Community and Favorites; Insert from Library
# and Community Library open the same window; the Floating layout. The
# Linux test is tools/ui-browse-components-test.sh.
add_test(NAME app.ui-windows-browse-components
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-browse-components-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-browse-components"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-browse-components PROPERTIES TIMEOUT 360)
