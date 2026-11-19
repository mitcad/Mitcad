# SPDX-License-Identifier: MIT
# Joint's picks under another component than the active one
# (mitcad#1856, GitHub mitcad/Mitcad#344): the question offers the
# component that holds both, Make it there mounts the sub-assembly
# whole, Don't ask again sets the preference, and Preferences > Assembly
# sets it back to Ask; picks inside one sub-assembly with the root active
# (mitcad#2027). The Linux test's shared input script
# (tools/ui-joint-holder-test.sh).
add_test(NAME app.ui-windows-joint-holder
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-joint-holder-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-joint-holder"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-joint-holder PROPERTIES TIMEOUT 600)
