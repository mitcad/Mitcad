# SPDX-License-Identifier: MIT
# Two bearings on one shaft joined by a Rigid joint or a Rigid Group move
# together (mitcad#2274): the shared steps of
# tools/ui-carriages-together-test.sh in the Qt frontend, on the reported
# design of the private test files (MITCAD_MITCAD_REPORTED); skipped
# without it.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-carriages-together
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-carriages-together-test.sh"
            "$<TARGET_FILE:mitcad>" "${CMAKE_BINARY_DIR}/ui-macos-carriages-together")
  set_tests_properties(app.ui-macos-carriages-together PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
