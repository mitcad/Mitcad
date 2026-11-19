# SPDX-License-Identifier: MIT
# The Boost development button (mitcad#1990): the shared steps of
# tools/ui-boost-button-test.sh in the Qt frontend's Docked layout (the
# Floating layout's macOS toolbar row has no Boost button): a click logs
# the Ko-fi link, the × hides it, the Settings window shows it again.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-boost-button
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-boost-button-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-boost-button")
  set_tests_properties(app.ui-macos-boost-button PROPERTIES
    TIMEOUT 360 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
