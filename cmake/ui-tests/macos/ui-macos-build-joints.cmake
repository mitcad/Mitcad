# SPDX-License-Identifier: MIT
# Joint conditions of profiles along sketch lines (mitcad#2200): the shared
# steps of tools/ui-build-joints-test.sh in the Qt frontend, with a
# synthetic BUILD library fetched from a local git repository.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-build-joints
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-build-joints-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-build-joints")
  set_tests_properties(app.ui-macos-build-joints PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
