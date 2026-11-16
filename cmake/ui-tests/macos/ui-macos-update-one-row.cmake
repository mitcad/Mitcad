# SPDX-License-Identifier: MIT
# One update for the whole application (mitcad#1890): the shared steps of
# tools/ui-update-one-row-test.sh in the Qt frontend with the update tests'
# fake offer (the test driver's step update-fake-offer, in any build).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-update-one-row
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-update-one-row-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-update-one-row")
  set_tests_properties(app.ui-macos-update-one-row PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
