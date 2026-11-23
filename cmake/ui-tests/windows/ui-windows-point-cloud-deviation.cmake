# SPDX-License-Identifier: MIT
# Point clouds and Point Cloud Deviation (mitcad#2240): the shared steps of
# tools/ui-scripts/point-cloud-deviation.mitcad-ui, a scan inserted beside a
# block, the deviation's statistics and coloured points, kept and saved
# (tools/ui-point-cloud-deviation-test.sh).
add_test(NAME app.ui-windows-point-cloud-deviation
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-point-cloud-deviation-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-point-cloud-deviation"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-point-cloud-deviation PROPERTIES TIMEOUT 600)
