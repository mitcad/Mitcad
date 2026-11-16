# SPDX-License-Identifier: MIT
# A joint edited with its Offset emptied while typing (mitcad#1860): the
# shared steps of tools/ui-joint-edit-empty-offset-test.sh in the Qt
# frontend, the joint and the rigid group after it held in the edit's
# preview and nothing moved while the Offset is empty.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-joint-edit-empty-offset
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-joint-edit-empty-offset-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-joint-edit-empty-offset")
  set_tests_properties(app.ui-macos-joint-edit-empty-offset PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
