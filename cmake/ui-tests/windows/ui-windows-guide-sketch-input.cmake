# SPDX-License-Identifier: MIT
# A tutorial's guide steps on the learner's sketch input (mitcad#2440): a
# plane, a drawing tool's point by its snap and a value box's value, each
# held back when missed. The Linux test's shared input script
# (tools/ui-guide-sketch-input-test.sh).
add_test(NAME app.ui-windows-guide-sketch-input
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-guide-sketch-input-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-guide-sketch-input"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-guide-sketch-input PROPERTIES TIMEOUT 240)
