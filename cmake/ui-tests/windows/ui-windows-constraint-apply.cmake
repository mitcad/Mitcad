# SPDX-License-Identifier: MIT
# Constraint, one assembly constraint at a time (mitcad#2293): Apply adds
# one and keeps the panel open with its type, OK adds and closes, each its
# own feature and undo step; a constraint of one row edited in the
# Constraint panel. The Linux test's shared input script
# (tools/ui-constraint-apply-test.sh).
add_test(NAME app.ui-windows-constraint-apply
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-constraint-apply-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-constraint-apply"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-constraint-apply PROPERTIES TIMEOUT 300)
