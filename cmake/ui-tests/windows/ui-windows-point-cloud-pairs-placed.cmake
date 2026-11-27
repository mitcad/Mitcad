# SPDX-License-Identifier: MIT
# Point pairs on bodies of a placed component (mitcad#2566): the shared
# steps of tools/ui-scripts/point-cloud-pairs-placed.mitcad-ui, a scan and a
# mesh aligned onto a block of a component placed turned and moved by three
# point pairs clicked in the view and the best fit
# (tools/ui-point-cloud-pairs-placed-test.sh).
add_test(NAME app.ui-windows-point-cloud-pairs-placed
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-point-cloud-pairs-placed-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-point-cloud-pairs-placed"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-point-cloud-pairs-placed PROPERTIES TIMEOUT 900)
