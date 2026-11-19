# SPDX-License-Identifier: MIT
# The Manage tab's Recompute All, Units and Plastic Rules (mitcad#1982):
# the shared steps of tools/ui-manage-commands-test.sh in the Qt frontend,
# each command run from its group's menu on the MANAGE tab.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-manage-commands
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-manage-commands-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-manage-commands")
  set_tests_properties(app.ui-macos-manage-commands PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
