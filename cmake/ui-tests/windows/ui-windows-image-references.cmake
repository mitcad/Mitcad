# SPDX-License-Identifier: MIT
# Image references (mitcad#782): an image file placed on a plate's top
# face, calibrated by two clicks on it and a typed distance, edited in
# its panel (opacity, Through Bodies), hidden, saved and shown again.
add_test(NAME app.ui-windows-image-references
  COMMAND powershell -NoProfile -ExecutionPolicy Bypass
          -File "${CMAKE_SOURCE_DIR}/tools/ui-windows-image-references-test.ps1"
          -App "$<TARGET_FILE:mitcad>" -Out "${CMAKE_BINARY_DIR}/ui-windows-image-references"
          -QtBin "$<TARGET_FILE_DIR:Qt6::Core>")
set_tests_properties(app.ui-windows-image-references PROPERTIES TIMEOUT 300)
