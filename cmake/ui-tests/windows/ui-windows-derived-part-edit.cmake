# SPDX-License-Identifier: MIT
# The edit panel of a derived part: a BUILD profile of a synthetic
# library (git) opened from its timeline cell, its length and row
# changed in one undo step, Undo; a feature without a panel says so in
# plain words (mitcad#1812). The Linux test's shared input script
# (tools/ui-derived-part-edit-test.sh).
add_test(NAME app.ui-windows-derived-part-edit
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-derived-part-edit-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-derived-part-edit"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-derived-part-edit PROPERTIES TIMEOUT 300)
