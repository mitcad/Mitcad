# SPDX-License-Identifier: MIT
# Preferences › Display › Toolbar labels (mitcad#1936): the labels under the
# ribbon's icons in the Floating and the Docked layout, the SOLID tab's
# Create group's labels and the groups folding from the right; the Linux
# test's shared input script (tools/ui-ribbon-labels-test.sh).
add_test(NAME app.ui-windows-ribbon-labels
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-ribbon-labels-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-ribbon-labels"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-ribbon-labels PROPERTIES TIMEOUT 360)
