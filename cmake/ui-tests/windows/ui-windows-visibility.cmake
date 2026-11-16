# SPDX-License-Identifier: MIT
# Light bulbs in every mode (mitcad#295): a body hidden while Create
# Sketch asks for a plane lets the face behind it be picked; during
# Extrude a body hidden stays so on Cancel and is no part of OK's undo
# step; an edit's roll-back keeps it too. Create Sketch's origin planes
# in a design with components too, behind its faces (mitcad#844).
add_test(NAME app.ui-windows-visibility
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-visibility-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-visibility"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-visibility PROPERTIES TIMEOUT 300)
