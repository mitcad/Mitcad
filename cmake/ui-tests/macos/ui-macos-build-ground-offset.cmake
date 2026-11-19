# SPDX-License-Identifier: MIT
# A BUILD profile placed off a face by Offset and Rotate and edited with
# Placement (mitcad#2046): the shared steps of
# tools/ui-build-ground-offset-test.sh in the Qt frontend, with a synthetic
# BUILD library fetched from a local git repository.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-build-ground-offset
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-build-ground-offset-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-build-ground-offset")
  set_tests_properties(app.ui-macos-build-ground-offset PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
