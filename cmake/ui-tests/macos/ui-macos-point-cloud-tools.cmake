# SPDX-License-Identifier: MIT
# Point cloud tools (mitcad#2516): the shared steps of
# tools/ui-point-cloud-tools-test.sh in the Qt frontend, a scan filtered,
# aligned onto a block with its remaining deviation stated and rebuilt as a
# mesh body, saved.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-point-cloud-tools
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-point-cloud-tools-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-point-cloud-tools")
  set_tests_properties(app.ui-macos-point-cloud-tools PROPERTIES
    TIMEOUT 600 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
