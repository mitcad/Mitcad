# SPDX-License-Identifier: MIT
# Edit Options of an inserted library part (mitcad#1929): the shared steps
# of tools/ui-edit-options-component-test.sh in the Qt frontend, with a
# synthetic library fetched from a local git repository.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-edit-options-component
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-edit-options-component-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-edit-options-component")
  set_tests_properties(app.ui-macos-edit-options-component PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
