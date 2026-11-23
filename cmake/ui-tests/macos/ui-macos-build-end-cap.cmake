# SPDX-License-Identifier: MIT
# End Cap (mitcad#2243): the shared steps of tools/ui-build-end-cap-test.sh
# in the Qt frontend, with a synthetic steel tube library fetched from a
# local git repository.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-build-end-cap
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-build-end-cap-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-build-end-cap")
  set_tests_properties(app.ui-macos-build-end-cap PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
