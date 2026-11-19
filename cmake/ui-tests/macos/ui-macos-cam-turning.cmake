# SPDX-License-Identifier: MIT
# CAM turning in the CNC group (mitcad#1869): the shared steps of
# tools/ui-cam-turning-test.sh in the Qt frontend, a lathe setup, five
# turning operations from their panels, Generate, Simulate and Post
# Process… to the Fanuc turning dialect.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-cam-turning
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-cam-turning-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-cam-turning")
  set_tests_properties(app.ui-macos-cam-turning PROPERTIES
    TIMEOUT 480 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
