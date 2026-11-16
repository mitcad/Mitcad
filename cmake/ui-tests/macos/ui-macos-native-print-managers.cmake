# SPDX-License-Identifier: MIT
# The native print managers by place (mitcad#1158), in a synthetic project
# and in a fresh application without a design. Uses the shared Aqua lock.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND MITCAD_MACOS_NATIVE_UI
   AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-native-print-managers
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-native-print-managers-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-native-print-managers")
  set_tests_properties(app.ui-macos-native-print-managers PROPERTIES
    TIMEOUT 900 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
