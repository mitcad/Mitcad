# SPDX-License-Identifier: MIT
# A command panel set aside hides its handles (mitcad#1844): the shared
# steps of tools/ui-joint-suspended-handles-test.sh in the Qt frontend,
# Joint's Animate with its handles hidden and a drag where one was moving
# nothing, the handles back after Esc.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-joint-suspended-handles
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-joint-suspended-handles-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-joint-suspended-handles")
  set_tests_properties(app.ui-macos-joint-suspended-handles PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
