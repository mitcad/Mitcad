# SPDX-License-Identifier: MIT
# A base view with many warnings (mitcad#2507, GitHub mitcad/Mitcad#694):
# the shared steps of tools/ui-drawing-many-warnings-test.sh in the Qt
# frontend.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-drawing-many-warnings
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-drawing-many-warnings-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-drawing-many-warnings")
  set_tests_properties(app.ui-macos-drawing-many-warnings PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
