# SPDX-License-Identifier: MIT
# An Unfold refused by a fillet on a surface edge (mitcad#1942): the shared
# steps of tools/ui-sheet-unfold-fillet-test.sh in the Qt frontend, the
# edited Unfold's panel naming the fillet in words, without ids.
if(APPLE AND BUILD_TESTING AND MITCAD_MACOS_UI_TESTS AND TARGET mitcad AND TARGET mitcad-cli AND BASH_EXECUTABLE)
  add_test(NAME app.ui-macos-sheet-unfold-fillet
    COMMAND "${BASH_EXECUTABLE}" "${CMAKE_SOURCE_DIR}/tools/ui-macos-sheet-unfold-fillet-test.sh"
            "$<TARGET_FILE:mitcad>" "$<TARGET_FILE:mitcad-cli>" "${CMAKE_BINARY_DIR}/ui-macos-sheet-unfold-fillet")
  set_tests_properties(app.ui-macos-sheet-unfold-fillet PROPERTIES
    TIMEOUT 300 SKIP_RETURN_CODE 77 RESOURCE_LOCK macos_desktop)
endif()
