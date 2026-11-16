# SPDX-License-Identifier: MIT
# A project's web viewer (mitcad#423): File > Project > Web Viewer
# writes both designs' pages and the index in one version; a Save
# writes the design's page again; Project Settings' Update on each Save
# off leaves it.
add_test(NAME app.ui-windows-web-viewer
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-web-viewer-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-web-viewer"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-web-viewer PROPERTIES TIMEOUT 600)
