# SPDX-License-Identifier: MIT
# Gear (mitcad#472): a spur gear from the panel, a rack meshing it.
add_test(NAME app.ui-windows-gear
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-gear-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-gear"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-gear PROPERTIES TIMEOUT 300)
