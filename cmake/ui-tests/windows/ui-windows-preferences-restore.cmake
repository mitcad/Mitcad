# SPDX-License-Identifier: MIT
# Preferences > Restore Defaults (mitcad#1887, GitHub mitcad/Mitcad#357):
# Current Group puts only the page shown back to its defaults, All
# Settings every page, each after a question, while a recent file and a
# library source stay. The Linux test's shared input script
# (tools/ui-preferences-restore-test.sh).
add_test(NAME app.ui-windows-preferences-restore
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-preferences-restore-test.ps1"
          -App "$<TARGET_FILE:mitcad>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-preferences-restore"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-preferences-restore PROPERTIES TIMEOUT 300)
