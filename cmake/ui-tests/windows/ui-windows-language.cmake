# SPDX-License-Identifier: MIT
# The interface language (mitcad#1460): English by default and with
# --language en, Finnish with --language fi (the tabs, the command search
# by the English name, the Language preference), the pseudo-language
# with no group caption cut. The Linux test is tools/ui-language-test.sh.
add_test(NAME app.ui-windows-language
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-language-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-language"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-language PROPERTIES TIMEOUT 600)
