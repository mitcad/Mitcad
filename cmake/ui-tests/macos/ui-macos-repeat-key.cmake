# SPDX-License-Identifier: MIT
# The command Repeat's own key (mitcad#2052): the shared steps of
# tools/ui-repeat-key-test.sh in the Qt frontend, Space given to Repeat in
# Keyboard Shortcuts repeating Extrude in the 3D view only.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-repeat-key
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-repeat-key-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-repeat-key")
  set_tests_properties(app.ui-macos-repeat-key PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
