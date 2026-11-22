# SPDX-License-Identifier: MIT
# The hatch of a large body's section caps lies in the section plane, 0
# within 0.000001 mm by the viewer's section look (mitcad#2146); the Linux
# test's shared input script.
add_test(NAME app.ui-windows-section-caps
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-section-caps-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Cli "$<TARGET_FILE:mitcad-cli>"
          -Out "${CMAKE_BINARY_DIR}/ui-windows-section-caps"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-section-caps PROPERTIES TIMEOUT 360)
