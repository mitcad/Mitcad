# SPDX-License-Identifier: MIT
# The edit panel of a derived part (mitcad#1812): the shared steps of
# tools/ui-derived-part-edit-test.sh in the Qt frontend, with a synthetic
# BUILD library fetched from a local git repository.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-derived-part-edit
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-derived-part-edit-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-derived-part-edit")
  set_tests_properties(app.ui-macos-derived-part-edit PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
