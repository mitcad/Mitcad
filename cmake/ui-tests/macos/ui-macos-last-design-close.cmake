# SPDX-License-Identifier: MIT
# Closing the last design (mitcad#2142): the shared steps of
# tools/ui-last-design-close-test.sh in the Qt frontend, File › Close of the
# only design showing the Start page with the preference off (the default)
# and quitting MitCad with it on, kept over a restart.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-last-design-close
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-last-design-close-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-last-design-close")
  set_tests_properties(app.ui-macos-last-design-close PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
