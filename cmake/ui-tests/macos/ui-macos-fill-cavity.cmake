# SPDX-License-Identifier: MIT
# Fill Cavity (mitcad#1818): the shared steps of tools/ui-fill-cavity-test.sh
# in the Qt frontend, a blind slit filled up to a plane and an open groove
# as the seed refused in words.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-fill-cavity
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-fill-cavity-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-fill-cavity")
  set_tests_properties(app.ui-macos-fill-cavity PROPERTIES
    TIMEOUT 660 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
