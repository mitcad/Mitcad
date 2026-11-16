# SPDX-License-Identifier: MIT
# Move to Component on a rigid group joined to a shaft (mitcad#1847): the
# shared steps of tools/ui-move-occurrence-group-test.sh in the Qt
# frontend, every unavailable parent's reasons under it and a rigid group
# moved as one.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-move-occurrence-group
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-move-occurrence-group-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-move-occurrence-group")
  set_tests_properties(app.ui-macos-move-occurrence-group PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
