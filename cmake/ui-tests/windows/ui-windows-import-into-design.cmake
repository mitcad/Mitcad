# SPDX-License-Identifier: MIT
# Design files of other formats into the open design as components
# (mitcad#1894): two .ipt test parts imported at once into a design
# (one Where to Import, side by side, one undo step, each file's
# report), Undo taking both out, then one as a new design in a tab of
# its own. The Linux test's shared input script
# (tools/ui-import-into-design-test.sh).
if(TARGET test_exchange)
  add_test(NAME app.ui-windows-import-into-design
    COMMAND powershell -NoProfile -ExecutionPolicy Bypass
            -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-import-into-design-test.ps1"
            -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
            -TestExchange "$<TARGET_FILE:test_exchange>"
            -Out "${CMAKE_BINARY_DIR}/ui-windows-import-into-design"
            -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
  set_tests_properties(app.ui-windows-import-into-design PROPERTIES TIMEOUT 600)
endif()
