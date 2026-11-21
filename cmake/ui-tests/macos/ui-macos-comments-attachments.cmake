# SPDX-License-Identifier: MIT
# Images on comments and replies (mitcad#2011): the shared steps of
# tools/ui-comments-attachments-test.sh in the Qt frontend, Capture and
# Annotate…, Reply… with Attach Image…, the thumbnails and the viewer, kept
# with the design.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-comments-attachments
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-comments-attachments-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-comments-attachments")
  set_tests_properties(app.ui-macos-comments-attachments PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
