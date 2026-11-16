# SPDX-License-Identifier: MIT
# The selection card's Create row (mitcad#619): a profile selected
# offers Extrude, Revolve and Sweep, the card's Extrude starts with
# the profile, a cylinder's round face offers Thread first, a cleared
# selection nothing.
add_test(NAME app.ui-windows-selection-create
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-selection-create-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-selection-create"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-selection-create PROPERTIES TIMEOUT 300)
