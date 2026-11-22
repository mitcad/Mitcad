# SPDX-License-Identifier: MIT
# Open Render and return to Design in both shared Qt layouts (mitcad#2080).
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_SHARED_QT_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-workspace-render-access
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-workspace-render-access-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-workspace-render-access")
  set_tests_properties(app.ui-macos-workspace-render-access PROPERTIES
    TIMEOUT 240 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
