# SPDX-License-Identifier: MIT
# Preferences › Export Settings and Import Settings (mitcad#1889): the
# shared steps of tools/ui-preferences-transfer-test.sh in the Settings
# window of the Qt frontend: a setting, a shortcut and a navigation bar pin
# exported, put back to their defaults and imported again.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-preferences-transfer
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-preferences-transfer-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-preferences-transfer")
  set_tests_properties(app.ui-macos-preferences-transfer PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
