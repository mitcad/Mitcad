# SPDX-License-Identifier: MIT
# Preferences' search (mitcad#2074): the shared steps of
# tools/ui-preferences-search-test.sh in the Settings window of the Qt
# frontend, its page list counting and greying, the settings highlighted,
# Return, Esc, a sub-tab and the command search's way in.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-preferences-search
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-preferences-search-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-preferences-search")
  set_tests_properties(app.ui-macos-preferences-search PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
