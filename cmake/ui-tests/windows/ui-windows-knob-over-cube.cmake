# SPDX-License-Identifier: MIT
# A click on a manipulator's knob over the orientation cube (mitcad#1105):
# Fillet's arrow panned onto the cube's top face and clicked without a
# drag keeps the click, the camera unchanged; without the handle the
# cube turns the view.
add_test(NAME app.ui-windows-knob-over-cube
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-knob-over-cube-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-knob-over-cube"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-knob-over-cube PROPERTIES TIMEOUT 300)
