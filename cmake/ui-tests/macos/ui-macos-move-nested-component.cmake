# SPDX-License-Identifier: MIT
# Move/Copy, Move Object Components, on a component three levels deep
# picked by its Browser row with the root active (mitcad#2302): the shared
# steps of tools/ui-move-nested-component-test.sh in the Qt frontend, the
# move going into the component the picked one is placed in.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-move-nested-component
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-move-nested-component-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-move-nested-component")
  set_tests_properties(app.ui-macos-move-nested-component PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
