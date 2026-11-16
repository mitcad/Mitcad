# SPDX-License-Identifier: MIT
# Preferences › Restore Defaults (mitcad#1887): the shared steps of
# tools/ui-preferences-restore-test.sh in the Settings window of the Qt
# frontend, Current Group and All Settings, each after a question, the panes
# shown again with the defaults and a recent file and a library source kept.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-preferences-restore
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-preferences-restore-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-preferences-restore")
  set_tests_properties(app.ui-macos-preferences-restore PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
