# SPDX-License-Identifier: MIT
# Save of a new design (mitcad#2183): the shared steps of
# tools/ui-new-design-save-test.sh in the Qt frontend, with a remote
# project (a local git repository following a local bare repository): the
# question On this computer or To a project, both answers, Remember my
# choice and its reset in Settings › General.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-new-design-save
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-new-design-save-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-new-design-save")
  set_tests_properties(app.ui-macos-new-design-save PROPERTIES
    TIMEOUT 420 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
