# SPDX-License-Identifier: MIT
# The design's origin shown and hidden with the Origin folder's eye and
# with View > Show > Origins, read from the view (mitcad#1642). The
# Linux test's shared input script (tools/ui-origin-show-test.sh).
add_test(NAME app.ui-windows-origin-show
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-origin-show-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-origin-show"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-origin-show PROPERTIES TIMEOUT 300)
