# SPDX-License-Identifier: MIT
# End Cap (mitcad#2243): a click near each end of a steel tube chooses it,
# both ends are capped outward in one undo step, one is switched to Inward
# and the lists show the plates. The Linux test's shared input script
# (tools/ui-build-end-cap-test.sh).
add_test(NAME app.ui-windows-build-end-cap
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-end-cap-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-build-end-cap"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-build-end-cap PROPERTIES TIMEOUT 300)
