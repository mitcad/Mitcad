# SPDX-License-Identifier: MIT
# The Browser's Annotations folder's eye (mitcad#2041): the shared steps of
# tools/ui-annotations-folder-eye-test.sh in the Qt frontend, every
# annotation hidden and shown in the view and kept with the design.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-annotations-folder-eye
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-annotations-folder-eye-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-annotations-folder-eye")
  set_tests_properties(app.ui-macos-annotations-folder-eye PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
