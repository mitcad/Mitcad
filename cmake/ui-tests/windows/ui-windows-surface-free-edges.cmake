# SPDX-License-Identifier: MIT
# A surface body's free edges picked with no command, and Extend's Whole
# Opening (mitcad#1955, GitHub mitcad/Mitcad#407): with no command a free
# edge hovers and selects as an edge, Extend started then takes it as its
# whole opening, and with Whole Opening off the single edge extends. On the
# reported design of the private test files (MITCAD_MITCAD_REPORTED);
# skipped without it. The Linux test's shared input script
# (tools/ui-surface-free-edges-test.sh).
add_test(NAME app.ui-windows-surface-free-edges
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-surface-free-edges-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-surface-free-edges"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-surface-free-edges PROPERTIES
  TIMEOUT 1000 SKIP_RETURN_CODE 77)
