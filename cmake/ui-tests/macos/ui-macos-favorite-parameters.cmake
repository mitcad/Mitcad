# SPDX-License-Identifier: MIT
# Favorite Parameters (mitcad#2203): the shared steps of
# tools/ui-favorite-parameters-test.sh in the Qt frontend, in the Floating
# and Docked layouts.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-favorite-parameters
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-favorite-parameters-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-favorite-parameters")
  set_tests_properties(app.ui-macos-favorite-parameters PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
