# SPDX-License-Identifier: MIT
# A Cloud project with a slow server (mitcad#1895): the shared steps of
# tools/ui-slow-remote-test.sh in the Qt frontend, its server a local bare
# repository behind an ssh that waits 5 s per connection; New Folder, Save
# to Project and Save recorded at once and sent in the background, the UI
# thread never stalling.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-slow-remote
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-slow-remote-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-slow-remote")
  set_tests_properties(app.ui-macos-slow-remote PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
