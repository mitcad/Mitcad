# SPDX-License-Identifier: MIT
# Edit Parameters sorted by a column (mitcad#1866): the shared steps of
# tools/ui-parameters-sort-test.sh in the Qt frontend, Name's and Value's
# headers both ways and back to the creation order, the choice kept, an
# edited and an added row selected in their place.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-parameters-sort
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-parameters-sort-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-parameters-sort")
  set_tests_properties(app.ui-macos-parameters-sort PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
