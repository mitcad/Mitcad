# SPDX-License-Identifier: MIT
# Convert to Features (mitcad#1815): a timber member from a library
# made in the test, joined to a plate, converted from its row's menu;
# its sketch and extrusion in the timeline, the section's width
# changed, the joint holding. The Linux test's shared input script
# (tools/ui-convert-features-test.sh).
add_test(NAME app.ui-windows-convert-features
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-convert-features-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-convert-features"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-convert-features PROPERTIES TIMEOUT 300)
