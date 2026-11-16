# SPDX-License-Identifier: MIT
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-render-recovery
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-render-recovery-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-render-recovery" qt)
  set_tests_properties(app.ui-macos-render-recovery PROPERTIES
    TIMEOUT 780 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
