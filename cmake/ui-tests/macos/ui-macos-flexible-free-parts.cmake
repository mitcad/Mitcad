# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE
   AND TARGET mitcad-cli)
  # Flexible on a subassembly without joints of its own (mitcad#2016):
  # Gantry:1 made Flexible from its row's menu and its free Joiner dragged
  # along a joint of the root, alone (the test driver's script, shared with
  # the Linux and Windows tests).
  add_test(NAME app.ui-macos-flexible-free-parts
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-flexible-free-parts-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-flexible-free-parts")
  set_tests_properties(app.ui-macos-flexible-free-parts PROPERTIES TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
