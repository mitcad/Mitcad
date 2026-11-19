# SPDX-License-Identifier: MIT
# Profile along sketch lines (mitcad#1965): the shared steps of
# tools/ui-build-lines-test.sh in the Qt frontend, with a synthetic BUILD
# library fetched from a local git repository.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-build-lines
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-build-lines-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-build-lines")
  set_tests_properties(app.ui-macos-build-lines PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
