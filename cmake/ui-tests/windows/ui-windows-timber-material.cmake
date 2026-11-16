# SPDX-License-Identifier: MIT
# A member placed with Timber is made of its library's wood: Physical
# Properties shows Spruce and the mass its density gives, never Steel
# (mitcad#1817). The Linux test's shared input script
# (tools/ui-timber-material-test.sh).
add_test(NAME app.ui-windows-timber-material
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-timber-material-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-timber-material"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-timber-material PROPERTIES TIMEOUT 300)
