# SPDX-License-Identifier: MIT
# Groups of user parameters in Edit Parameters (mitcad#2180): the shared
# steps of tools/ui-parameter-groups-test.sh in the Qt frontend, a group
# made, two parameters moved into it by the context menu, renamed in place,
# collapsed and deleted, its parameters back under User Parameters.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-parameter-groups
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-parameter-groups-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-parameter-groups")
  set_tests_properties(app.ui-macos-parameter-groups PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
