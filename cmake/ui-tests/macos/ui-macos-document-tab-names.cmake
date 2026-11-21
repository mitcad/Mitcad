# SPDX-License-Identifier: MIT
# The document tabs' names (mitcad#2060): the shared steps of
# tools/ui-document-tab-names-test.sh in the Qt frontend: Bracket and
# Untitled shown whole, a 60-character name elided.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-document-tab-names
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-document-tab-names-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-document-tab-names")
  set_tests_properties(app.ui-macos-document-tab-names PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
