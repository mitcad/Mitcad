# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND TARGET mitcad-cli)
  # A design linked with its components made Flexible (mitcad#2209):
  # Flexible offered on the linked sub-assembly's Browser row and its
  # slider part dragged along the design's own joint, for that occurrence
  # alone (the test driver's script, shared with the Linux and Windows
  # tests).
  add_test(NAME app.ui-macos-linked-flexible
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-linked-flexible-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-linked-flexible")
  set_tests_properties(app.ui-macos-linked-flexible PROPERTIES TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
