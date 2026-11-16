# SPDX-License-Identifier: MIT
# Edit Text from inside a letter (mitcad#1369, GitHub mitcad/Mitcad#158):
# zoomed in, a right-click and a double-click inside the I's stroke and
# the o's stroke and counter offer and open the text's Edit Text, and a
# QR code's finder pattern (its dark centre and the light ring round it)
# Edit QR Code. The Linux test is
# tools/ui-text-region-edit-test.sh.
add_test(NAME app.ui-windows-text-region-edit
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-text-region-edit-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-text-region-edit"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-text-region-edit PROPERTIES TIMEOUT 300)
