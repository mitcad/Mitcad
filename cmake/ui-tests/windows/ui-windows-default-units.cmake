# SPDX-License-Identifier: MIT
# Default units for new designs (mitcad#1036): set to in, New is in
# inches (Units: in, a typed extrude distance of 1 is an inch), the
# untitled design at a new start too, an opened mm design keeps mm;
# back to mm, New is in mm.
add_test(NAME app.ui-windows-default-units
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-default-units-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-default-units"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-default-units PROPERTIES TIMEOUT 300)
