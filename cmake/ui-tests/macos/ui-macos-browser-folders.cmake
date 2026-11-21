# SPDX-License-Identifier: MIT
# Folders in the Browser (mitcad#2103): the shared steps of
# tools/ui-browser-folders-test.sh in the Qt frontend, components and
# bodies into folders and out, kept with the design.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-browser-folders
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-browser-folders-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-browser-folders")
  set_tests_properties(app.ui-macos-browser-folders PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
