# SPDX-License-Identifier: MIT
# 3MF import (mitcad#2156): a 3MF file of MitCad's export opened and
# imported with File > Import, its mesh body and its component of two
# occurrences found. The Linux test's shared input script
# (tools/ui-3mf-import-test.sh).
add_test(NAME app.ui-windows-3mf-import
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-3mf-import-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-3mf-import"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-3mf-import PROPERTIES TIMEOUT 300)
