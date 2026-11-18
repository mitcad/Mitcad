# SPDX-License-Identifier: MIT
# A profile centred across a side with two slots (mitcad#2002): a click
# near the side's centre line centres the profile across the side, the
# preview saying so; a click near a slot puts it on that slot. The Linux
# test's shared input script (tools/ui-build-side-middle-test.sh).
add_test(NAME app.ui-windows-build-side-middle
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-build-side-middle-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-build-side-middle"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-build-side-middle PROPERTIES TIMEOUT 300)
