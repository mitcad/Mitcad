# SPDX-License-Identifier: MIT
# Section Analysis through a pick (mitcad#1769): a hole's round face
# selected starts it with the plane through the hole's axis, shown at
# once; kept, its Angle turns it about that axis without an Axis; two
# faces selected start it with nothing picked. The Linux test is
# tools/ui-section-hole-pick-test.sh.
add_test(NAME app.ui-windows-section-hole-pick
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-section-hole-pick-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-section-hole-pick"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-section-hole-pick PROPERTIES TIMEOUT 300)
