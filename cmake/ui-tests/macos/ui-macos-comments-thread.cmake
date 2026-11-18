# SPDX-License-Identifier: MIT
# A comment thread with an agent (mitcad#2010): the shared steps of
# tools/ui-comments-test.sh's second part in the Qt frontend, Add Comment
# with Whole Design in an empty design, an agent's long reply in full in
# the details pane, waiting for your answer, and Reply… showing the reply
# it answers.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-comments-thread
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-comments-thread-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-comments-thread")
  set_tests_properties(app.ui-macos-comments-thread PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
