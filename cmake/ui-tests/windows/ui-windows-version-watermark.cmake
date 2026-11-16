# SPDX-License-Identifier: MIT
# MitCad's version in the main window (mitcad#1209): the watermark's text
# and the window title, its place in the view's lower right (above the
# timeline in the Floating layout), a click there reaches the view, and
# Preferences › General hides and shows it.
add_test(NAME app.ui-windows-version-watermark
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-version-watermark-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-version-watermark"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>" -Version "${PROJECT_VERSION}")
set_tests_properties(app.ui-windows-version-watermark PROPERTIES TIMEOUT 300)
