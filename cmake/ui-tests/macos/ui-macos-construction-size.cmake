# SPDX-License-Identifier: MIT
# Construction planes drawn at the size and shape of what they come from
# (mitcad#1827): the shared steps of tools/ui-construction-size-test.sh in
# the Qt frontend, a plane saved with a size opening unchanged, one off a
# long narrow face a rectangle over it, a handle's drag and Reset Size.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-construction-size
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-construction-size-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-construction-size")
  set_tests_properties(app.ui-macos-construction-size PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
