# SPDX-License-Identifier: MIT
# Move to Component on a rigid group joined to a shaft (mitcad#1847,
# GitHub mitcad/Mitcad#334): every unavailable parent's reasons under
# it, the rest of a rigid group offered, several selected occurrences
# moved in one step. The Linux test's shared input script
# (tools/ui-move-occurrence-group-test.sh).
add_test(NAME app.ui-windows-move-occurrence-group
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-move-occurrence-group-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-move-occurrence-group"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-move-occurrence-group PROPERTIES TIMEOUT 300)
