# SPDX-License-Identifier: MIT
# The timeline filter's default, Active Component with its
# subcomponents, and the choice saved for the next start (mitcad#1662).
# The Linux test's shared input scripts
# (tools/ui-timeline-filter-default-test.sh).
add_test(NAME app.ui-windows-timeline-filter-default
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-timeline-filter-default-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-timeline-filter-default"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-timeline-filter-default PROPERTIES TIMEOUT 360)
